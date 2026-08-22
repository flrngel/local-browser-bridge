//! Persistent, exact-window ScreenCaptureKit sharing for macOS.
//!
//! This module deliberately has no legacy screenshot fallback. Starting a
//! share either binds an `SCStream` to the requested `(pid, CGWindowID)` or
//! fails. Frames are converted on ScreenCaptureKit's serial callback queue and
//! parked in a single latest-frame slot so a slow transport cannot create an
//! unbounded pixel-buffer backlog.

use std::ffi::c_void;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{Boolean, CFGetTypeID};
use core_foundation_sys::dictionary::{
    CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef,
};
use core_foundation_sys::number::{
    CFNumberGetTypeID, CFNumberGetValue, CFNumberRef, kCFNumberSInt32Type,
};
use image::RgbaImage;
use screencapturekit::cm::{CMSampleBuffer, CMSampleBufferExt, CMSampleBufferSCExt, SCFrameStatus};
use screencapturekit::prelude::{
    ErrorHandler, PixelFormat, SCContentFilter, SCShareableContent, SCStream,
    SCStreamConfiguration, SCStreamOutputType,
};

use super::{ComputerError, WindowDescriptor};

const MIN_FPS: u64 = 1;
const MAX_FPS: u64 = 10;
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const RECONFIGURED_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const CONTENT_ASPECT_TOLERANCE: f64 = 0.003;
const CONTENT_SIZE_TOLERANCE_POINTS: f64 = 1.0;
const BGRA_PIXEL_FORMAT: u32 = u32::from_be_bytes(*b"BGRA");
const FRAME_STATUS_ATTACHMENT_KEY: &str = "SCStreamUpdateFrameStatus";

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMSampleBufferGetSampleAttachmentsArray(
        sample_buffer: *mut c_void,
        create_if_necessary: Boolean,
    ) -> CFArrayRef;
}

unsafe extern "C" {
    fn mach_absolute_time() -> u64;
}

#[derive(Debug)]
pub(crate) struct NativeShareFrame {
    pub(crate) image: RgbaImage,
    pub(crate) source_sequence: u64,
    pub(crate) captured_at: Instant,
    geometry: CaptureGeometry,
}

pub(crate) fn bind_frame_geometry(frame: &NativeShareFrame, target: &mut WindowDescriptor) {
    target.x = frame.geometry.x;
    target.y = frame.geometry.y;
    target.width = frame.geometry.width;
    target.height = frame.geometry.height;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CaptureGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl From<&WindowDescriptor> for CaptureGeometry {
    fn from(target: &WindowDescriptor) -> Self {
        Self {
            x: target.x,
            y: target.y,
            width: target.width,
            height: target.height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutputDimensions {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeShareMetadata {
    pub(crate) backend: &'static str,
    pub(crate) native_stream: bool,
    pub(crate) system_indicator: bool,
    pub(crate) selection_mode: &'static str,
}

pub(crate) struct NativeShareCapture {
    stream: Option<SCStream>,
    filter: SCContentFilter,
    state: Arc<CaptureState>,
    window_id: String,
    target_pid: u32,
    geometry: CaptureGeometry,
    output_dimensions: OutputDimensions,
    fps: u64,
    stopped: bool,
}

impl NativeShareCapture {
    pub(crate) fn start(target: &WindowDescriptor, fps: u64) -> Result<Self, ComputerError> {
        if !(MIN_FPS..=MAX_FPS).contains(&fps) {
            return Err(ComputerError::new(
                "COMPUTER_CAPTURE_FAILED",
                format!("Native window sharing requires fps between {MIN_FPS} and {MAX_FPS}"),
            ));
        }
        if target.minimized || target.width == 0 || target.height == 0 {
            return Err(no_window(
                "The requested exact window is minimized or has no capturable area",
            ));
        }

        let window_id = target
            .id
            .parse::<u32>()
            .map_err(|_| no_window("The requested exact window has an invalid native id"))?;
        let target_pid = i32::try_from(target.pid)
            .map_err(|_| no_window("The requested exact window has an invalid process id"))?;

        let content = SCShareableContent::create()
            .with_exclude_desktop_windows(true)
            .with_on_screen_windows_only(true)
            .get()
            .map_err(|error| {
                capture_error(format!(
                    "ScreenCaptureKit could not enumerate windows. Grant Screen Recording to Local Computer Helper. {error}"
                ))
            })?;
        let window = content
            .windows()
            .into_iter()
            .find(|window| {
                window.window_id() == window_id
                    && window
                        .owning_application()
                        .is_some_and(|application| application.process_id() == target_pid)
            })
            .ok_or_else(|| {
                no_window(
                    "The requested (process, window) pair is not available to ScreenCaptureKit",
                )
            })?;

        let filter = SCContentFilter::create()
            .with_window(&window)
            .try_build()
            .map_err(|error| {
                capture_error(format!("Could not build exact-window filter: {error}"))
            })?;
        let geometry = CaptureGeometry::from(target);
        let output_dimensions = output_dimensions(&filter, target)?;
        let configuration = stream_configuration(output_dimensions, fps)?;

        let state = Arc::new(CaptureState::new(output_dimensions, geometry));
        let delegate_state = Arc::clone(&state);
        let delegate = ErrorHandler::new(move |error| {
            delegate_state.fail(capture_error(format!(
                "ScreenCaptureKit stopped the exact-window stream: {error}"
            )));
        });
        let mut stream = SCStream::new_with_delegate(&filter, &configuration, delegate);

        let handler_state = Arc::clone(&state);
        let handler_id = stream.add_output_handler(
            move |sample_buffer: CMSampleBuffer, output_type: SCStreamOutputType| {
                if output_type != SCStreamOutputType::Screen {
                    return;
                }
                match frame_status(&sample_buffer) {
                    Ok(status) if frame_status_is_complete(status) => {},
                    Ok(_) => return,
                    Err(detail) => {
                        handler_state.fail(capture_error(format!(
                            "Could not decode ScreenCaptureKit frame status: {detail}"
                        )));
                        return;
                    }
                }
                if !sample_buffer.is_data_ready() {
                    return;
                }

                let captured_at = Instant::now();
                let display_time = sample_buffer.display_time();
                let Some(pixel_buffer) = sample_buffer.image_buffer() else {
                    handler_state.fail(capture_error(
                        "ScreenCaptureKit frame has no pixel buffer",
                    ));
                    return;
                };
                let dimensions = match pixel_dimensions(pixel_buffer.width(), pixel_buffer.height())
                {
                    Ok(dimensions) => dimensions,
                    Err(error) => {
                        handler_state.fail(error);
                        return;
                    }
                };
                let Some(candidate) = handler_state.frame_candidate(display_time, dimensions)
                else {
                    return;
                };
                let result = if pixel_buffer.pixel_format() != BGRA_PIXEL_FORMAT {
                    Err(capture_error(format!(
                        "ScreenCaptureKit returned unexpected pixel format 0x{:08X}",
                        pixel_buffer.pixel_format()
                    )))
                } else {
                    pixel_buffer
                        .lock_read_only()
                        .map_err(|status| {
                            capture_error(format!(
                                "Could not lock ScreenCaptureKit pixel buffer (CoreVideo status {status})"
                            ))
                        })
                        .and_then(|guard| {
                            bgra_to_rgba(
                                guard.width(),
                                guard.height(),
                                guard.bytes_per_row(),
                                guard.as_slice(),
                            )
                            .map_err(capture_error)
                        })
                };

                match result {
                    Ok(image) => handler_state.store_frame(candidate, image, captured_at),
                    Err(error) => handler_state.fail_epoch(candidate.epoch, error),
                }
            },
            SCStreamOutputType::Screen,
        );
        if handler_id.is_none() {
            return Err(capture_error(
                "ScreenCaptureKit rejected the exact-window frame output handler",
            ));
        }

        stream.start_capture().map_err(|error| {
            capture_error(format!(
                "ScreenCaptureKit could not start exact-window sharing: {error}"
            ))
        })?;
        if let Err(error) = state.wait_for_first_frame(FIRST_FRAME_TIMEOUT) {
            let _ = stream.stop_capture();
            return Err(error);
        }

        Ok(Self {
            stream: Some(stream),
            filter,
            state,
            window_id: target.id.clone(),
            target_pid: target.pid,
            geometry,
            output_dimensions,
            fps,
            stopped: false,
        })
    }

    /// Revalidates the exact target before a frame can leave the native source.
    /// A geometry change updates the existing SCStream in place, preserving the
    /// share lease and OS capture indicator. The callback state is paused first
    /// so neither a parked frame nor an in-flight pre-update sample can cross
    /// into the new geometry epoch.
    pub(crate) fn prepare_for_target(
        &mut self,
        target: &WindowDescriptor,
    ) -> Result<bool, ComputerError> {
        if target.id != self.window_id || target.pid != self.target_pid {
            return Err(no_window(
                "The exact ScreenCaptureKit target changed process or window identity",
            ));
        }
        if target.minimized || target.width == 0 || target.height == 0 {
            return Err(no_window(
                "The exact ScreenCaptureKit target is minimized or has no capturable area",
            ));
        }

        let geometry = CaptureGeometry::from(target);
        if geometry == self.geometry {
            return Ok(false);
        }

        let output_dimensions = output_dimensions(&self.filter, target)?;
        let size_or_scale_changed = geometry.width != self.geometry.width
            || geometry.height != self.geometry.height
            || output_dimensions != self.output_dimensions;
        let configuration = size_or_scale_changed
            .then(|| stream_configuration(output_dimensions, self.fps))
            .transpose()?;
        let stream = configuration
            .as_ref()
            .map(|_| {
                self.stream
                    .as_ref()
                    .ok_or_else(|| capture_error("The ScreenCaptureKit stream is not running"))
            })
            .transpose()?;
        let epoch = self.state.begin_reconfiguration()?;
        let update_result = match (stream, configuration.as_ref()) {
            (Some(stream), Some(configuration)) => {
                stream.update_configuration(configuration).map_err(|error| {
                    capture_error(format!(
                        "ScreenCaptureKit could not update exact-window geometry in place: {error}"
                    ))
                })
            }
            (None, None) => {
                // A position-only move does not alter an exact-window stream's
                // pixel configuration, but it still advances the frame authority
                // epoch below so desktop-coordinate input cannot use old bounds.
                Ok(())
            }
            _ => Err(capture_error(
                "ScreenCaptureKit configuration update state became inconsistent",
            )),
        };
        if let Err(error) = update_result {
            self.state.fail(error.clone());
            return Err(error);
        }
        // SCStreamFrameInfo.displayTime uses the mach absolute-time clock. Take
        // the boundary only after updateConfiguration completes while the gate
        // is still paused. A sample displayed during the update can otherwise
        // arrive later and look new when capped/same-aspect dimensions repeat.
        // SAFETY: `mach_absolute_time` has no parameters or memory effects and
        // returns the host clock used by SCStreamFrameInfo.displayTime.
        let display_time_boundary = unsafe { mach_absolute_time() };
        self.state.finish_reconfiguration(
            epoch,
            display_time_boundary,
            output_dimensions,
            geometry,
        )?;
        self.geometry = geometry;
        self.output_dimensions = output_dimensions;
        Ok(true)
    }

    /// Takes the newest native frame if it is newer than the caller's source
    /// sequence. Taking rather than cloning keeps the native handoff bounded to
    /// one owned image even when the downstream transport is backpressured.
    pub(crate) fn latest_after(
        &self,
        last_source_sequence: u64,
    ) -> Result<Option<NativeShareFrame>, ComputerError> {
        self.state.take_latest_after(last_source_sequence)
    }

    pub(crate) fn stop(&mut self) -> Result<(), ComputerError> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;

        let result = self
            .stream
            .as_ref()
            .map_or(Ok(()), SCStream::stop_capture)
            .map_err(|error| {
                capture_error(format!(
                    "ScreenCaptureKit could not stop exact-window sharing cleanly: {error}"
                ))
            });
        self.stream.take();
        self.state.clear_latest();
        result
    }

    pub(crate) const fn metadata(&self) -> NativeShareMetadata {
        NativeShareMetadata {
            backend: "macos-screencapturekit-scstream",
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeShareCapture")
            .field("window_id", &self.window_id)
            .field("fps", &self.fps)
            .field("output_dimensions", &self.output_dimensions)
            .field("stopped", &self.stopped)
            .field("metadata", &self.metadata())
            .field("dropped_frames", &self.dropped_frames())
            .finish_non_exhaustive()
    }
}

struct CaptureState {
    inner: Mutex<CaptureStateInner>,
    changed: Condvar,
}

struct CaptureStateInner {
    latest: Option<NativeShareFrame>,
    last_sequence: u64,
    dropped_frames: u64,
    terminal_error: Option<ComputerError>,
    gate: FrameGate,
    reconfigured_frame_deadline: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameGate {
    Paused {
        epoch: u64,
    },
    Active {
        epoch: u64,
        display_time_boundary: Option<u64>,
        output_dimensions: OutputDimensions,
        geometry: CaptureGeometry,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameCandidate {
    epoch: u64,
    output_dimensions: OutputDimensions,
    geometry: CaptureGeometry,
}

impl CaptureState {
    fn new(output_dimensions: OutputDimensions, geometry: CaptureGeometry) -> Self {
        Self {
            inner: Mutex::new(CaptureStateInner {
                latest: None,
                last_sequence: 0,
                dropped_frames: 0,
                terminal_error: None,
                gate: FrameGate::Active {
                    epoch: 0,
                    display_time_boundary: None,
                    output_dimensions,
                    geometry,
                },
                reconfigured_frame_deadline: None,
            }),
            changed: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, CaptureStateInner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn frame_candidate(
        &self,
        display_time: Option<u64>,
        output_dimensions: OutputDimensions,
    ) -> Option<FrameCandidate> {
        let mut inner = self.lock();
        if inner.terminal_error.is_some() {
            return None;
        }
        let FrameGate::Active {
            epoch,
            display_time_boundary,
            output_dimensions: expected_dimensions,
            geometry,
        } = inner.gate
        else {
            inner.dropped_frames = inner.dropped_frames.saturating_add(1);
            return None;
        };
        let is_after_boundary = display_time_boundary
            .is_none_or(|boundary| display_time.is_some_and(|time| time > boundary));
        if output_dimensions != expected_dimensions || !is_after_boundary {
            inner.dropped_frames = inner.dropped_frames.saturating_add(1);
            return None;
        }
        Some(FrameCandidate {
            epoch,
            output_dimensions,
            geometry,
        })
    }

    fn store_frame(&self, candidate: FrameCandidate, image: RgbaImage, captured_at: Instant) {
        let mut inner = self.lock();
        if inner.terminal_error.is_some() {
            return;
        }
        let candidate_is_current = matches!(
            inner.gate,
            FrameGate::Active {
                epoch,
                output_dimensions,
                geometry,
                ..
            } if epoch == candidate.epoch
                && output_dimensions == candidate.output_dimensions
                && geometry == candidate.geometry
        );
        if !candidate_is_current {
            inner.dropped_frames = inner.dropped_frames.saturating_add(1);
            return;
        }
        let Some(sequence) = inner.last_sequence.checked_add(1) else {
            inner.terminal_error = Some(capture_error(
                "Native capture source sequence exhausted its numeric range",
            ));
            drop(inner);
            self.changed.notify_all();
            return;
        };
        inner.last_sequence = sequence;
        if inner.latest.is_some() {
            inner.dropped_frames = inner.dropped_frames.saturating_add(1);
        }
        inner.latest = Some(NativeShareFrame {
            image,
            source_sequence: sequence,
            captured_at,
            geometry: candidate.geometry,
        });
        inner.reconfigured_frame_deadline = None;
        drop(inner);
        self.changed.notify_all();
    }

    fn begin_reconfiguration(&self) -> Result<u64, ComputerError> {
        let mut inner = self.lock();
        if let Some(error) = inner.terminal_error.clone() {
            return Err(error);
        }
        let current_epoch = match inner.gate {
            FrameGate::Paused { epoch } | FrameGate::Active { epoch, .. } => epoch,
        };
        let epoch = current_epoch.checked_add(1).ok_or_else(|| {
            capture_error("Native capture geometry epoch exhausted its numeric range")
        })?;
        if inner.latest.take().is_some() {
            inner.dropped_frames = inner.dropped_frames.saturating_add(1);
        }
        inner.gate = FrameGate::Paused { epoch };
        inner.reconfigured_frame_deadline = None;
        drop(inner);
        self.changed.notify_all();
        Ok(epoch)
    }

    fn finish_reconfiguration(
        &self,
        epoch: u64,
        display_time_boundary: u64,
        output_dimensions: OutputDimensions,
        geometry: CaptureGeometry,
    ) -> Result<(), ComputerError> {
        self.finish_reconfiguration_at(
            epoch,
            display_time_boundary,
            output_dimensions,
            geometry,
            Instant::now(),
        )
    }

    fn finish_reconfiguration_at(
        &self,
        epoch: u64,
        display_time_boundary: u64,
        output_dimensions: OutputDimensions,
        geometry: CaptureGeometry,
        now: Instant,
    ) -> Result<(), ComputerError> {
        let mut inner = self.lock();
        if let Some(error) = inner.terminal_error.clone() {
            return Err(error);
        }
        if inner.gate != (FrameGate::Paused { epoch }) {
            return Err(capture_error(
                "ScreenCaptureKit geometry epoch changed during configuration update",
            ));
        }
        inner.gate = FrameGate::Active {
            epoch,
            display_time_boundary: Some(display_time_boundary),
            output_dimensions,
            geometry,
        };
        inner.reconfigured_frame_deadline = now.checked_add(RECONFIGURED_FRAME_TIMEOUT);
        drop(inner);
        self.changed.notify_all();
        Ok(())
    }

    fn fail_epoch(&self, epoch: u64, error: ComputerError) {
        let mut inner = self.lock();
        let current_epoch = match inner.gate {
            FrameGate::Paused { epoch } | FrameGate::Active { epoch, .. } => epoch,
        };
        if current_epoch == epoch && inner.terminal_error.is_none() {
            inner.terminal_error = Some(error);
            inner.latest = None;
        }
        drop(inner);
        self.changed.notify_all();
    }

    fn fail(&self, error: ComputerError) {
        let mut inner = self.lock();
        if inner.terminal_error.is_none() {
            inner.terminal_error = Some(error);
            inner.latest = None;
        }
        drop(inner);
        self.changed.notify_all();
    }

    fn wait_for_first_frame(&self, timeout: Duration) -> Result<(), ComputerError> {
        let inner = self.lock();
        let (inner, wait_result) = self
            .changed
            .wait_timeout_while(inner, timeout, |inner| {
                inner.latest.is_none() && inner.terminal_error.is_none()
            })
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(error) = inner.terminal_error.clone() {
            return Err(error);
        }
        if inner.latest.is_some() {
            return Ok(());
        }
        let detail = if wait_result.timed_out() {
            "timed out waiting for the first complete frame"
        } else {
            "ended before producing the first complete frame"
        };
        Err(capture_error(format!(
            "ScreenCaptureKit {detail}; the window may contain protected content or no longer be shareable"
        )))
    }

    fn take_latest_after(
        &self,
        last_source_sequence: u64,
    ) -> Result<Option<NativeShareFrame>, ComputerError> {
        let mut inner = self.lock();
        if let Some(error) = inner.terminal_error.clone() {
            return Err(error);
        }
        if inner
            .reconfigured_frame_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            let error = capture_error(
                "ScreenCaptureKit timed out waiting for a geometry-bound frame after updating the exact-window stream",
            );
            inner.terminal_error = Some(error.clone());
            inner.latest = None;
            return Err(error);
        }
        Ok(inner
            .latest
            .take()
            .filter(|frame| frame.source_sequence > last_source_sequence))
    }

    fn clear_latest(&self) {
        self.lock().latest = None;
    }

    fn dropped_frames(&self) -> u64 {
        self.lock().dropped_frames
    }
}

fn stream_configuration(
    output_dimensions: OutputDimensions,
    fps: u64,
) -> Result<SCStreamConfiguration, ComputerError> {
    let fps = u32::try_from(fps)
        .map_err(|_| capture_error("The requested native capture frame rate is out of range"))?;
    Ok(SCStreamConfiguration::new()
        .with_width(output_dimensions.width)
        .with_height(output_dimensions.height)
        .with_pixel_format(PixelFormat::BGRA)
        .with_shows_cursor(false)
        .with_queue_depth(3)
        .with_fps(fps))
}

fn output_dimensions(
    filter: &SCContentFilter,
    target: &WindowDescriptor,
) -> Result<OutputDimensions, ComputerError> {
    let content_rect = filter.content_rect();
    let (content_width, content_height) = geometry_aligned_content_size(
        content_rect.size.width,
        content_rect.size.height,
        target.width,
        target.height,
    );
    let (width, height) = capture_dimensions(
        content_width,
        content_height,
        f64::from(filter.point_pixel_scale()),
        target.width,
        target.height,
        super::MAX_CAPTURE_PIXELS,
    )
    .map_err(capture_error)?;
    Ok(OutputDimensions { width, height })
}

/// A retained content filter can briefly report its previous content rect
/// immediately after WindowServer publishes new target bounds. Prefer the
/// exact enumerated target for that transition instead of configuring a stale
/// aspect ratio. Stable filter geometry remains authoritative for fractional
/// point sizes on newer macOS releases.
fn geometry_aligned_content_size(
    content_width: f64,
    content_height: f64,
    target_width: u32,
    target_height: u32,
) -> (f64, f64) {
    let target_width = f64::from(target_width);
    let target_height = f64::from(target_height);
    let content_is_aligned = valid_positive(content_width)
        .zip(valid_positive(content_height))
        .is_some_and(|(width, height)| {
            ((width / height) - (target_width / target_height)).abs() <= CONTENT_ASPECT_TOLERANCE
                && (width - target_width).abs() <= CONTENT_SIZE_TOLERANCE_POINTS
                && (height - target_height).abs() <= CONTENT_SIZE_TOLERANCE_POINTS
        });
    if content_is_aligned {
        (content_width, content_height)
    } else {
        (target_width, target_height)
    }
}

fn pixel_dimensions(width: usize, height: usize) -> Result<OutputDimensions, ComputerError> {
    let width = u32::try_from(width)
        .map_err(|_| capture_error("ScreenCaptureKit frame width exceeds image limits"))?;
    let height = u32::try_from(height)
        .map_err(|_| capture_error("ScreenCaptureKit frame height exceeds image limits"))?;
    if width == 0 || height == 0 {
        return Err(capture_error(
            "ScreenCaptureKit returned a zero-sized pixel buffer",
        ));
    }
    Ok(OutputDimensions { width, height })
}

fn bgra_to_rgba(
    width: usize,
    height: usize,
    bytes_per_row: usize,
    bytes: &[u8],
) -> Result<RgbaImage, String> {
    let packed_row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| "Native frame row size overflowed".to_owned())?;
    if width == 0 || height == 0 {
        return Err("Native frame has zero width or height".to_owned());
    }
    if bytes_per_row < packed_row_bytes {
        return Err(format!(
            "Native frame stride {bytes_per_row} is smaller than packed row size {packed_row_bytes}"
        ));
    }
    let required_bytes = height
        .checked_mul(bytes_per_row)
        .ok_or_else(|| "Native frame buffer size overflowed".to_owned())?;
    if bytes.len() < required_bytes {
        return Err(format!(
            "Native frame buffer is truncated: expected at least {required_bytes} bytes, got {}",
            bytes.len()
        ));
    }

    let pixel_bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Native frame pixel count overflowed".to_owned())?;
    let mut rgba = Vec::with_capacity(pixel_bytes);
    for row in bytes.chunks_exact(bytes_per_row).take(height) {
        for pixel in row[..packed_row_bytes].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }

    let width =
        u32::try_from(width).map_err(|_| "Native frame width exceeds image limits".to_owned())?;
    let height =
        u32::try_from(height).map_err(|_| "Native frame height exceeds image limits".to_owned())?;
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "Native frame could not be represented as RGBA".to_owned())
}

/// Chooses the stream's pixel dimensions without creating oversized native
/// queues. ScreenCaptureKit exposes content geometry in points and, on macOS
/// 14+, a point-to-pixel scale. The Swift bridge returns zero geometry and a
/// 1x scale on macOS 13, where the validated window descriptor is the runtime
/// fallback. Capping here avoids allocating full Retina buffers only to resize
/// them again at the transport boundary.
fn capture_dimensions(
    content_width_points: f64,
    content_height_points: f64,
    point_pixel_scale: f64,
    fallback_width_points: u32,
    fallback_height_points: u32,
    max_pixels: u64,
) -> Result<(u32, u32), String> {
    if fallback_width_points == 0 || fallback_height_points == 0 || max_pixels == 0 {
        return Err("Native capture sizing has an empty fallback or pixel budget".to_owned());
    }

    let (width_points, height_points) = match (
        valid_positive(content_width_points),
        valid_positive(content_height_points),
    ) {
        (Some(width), Some(height)) => (width, height),
        _ => (
            f64::from(fallback_width_points),
            f64::from(fallback_height_points),
        ),
    };
    let pixel_scale = valid_positive(point_pixel_scale).unwrap_or(1.0);
    let native_width = width_points * pixel_scale;
    let native_height = height_points * pixel_scale;
    if !native_width.is_finite()
        || !native_height.is_finite()
        || native_width > f64::from(u32::MAX)
        || native_height > f64::from(u32::MAX)
    {
        return Err("ScreenCaptureKit reported out-of-range content geometry".to_owned());
    }

    let native_pixels = native_width * native_height;
    if !native_pixels.is_finite() || native_pixels <= 0.0 {
        return Err("ScreenCaptureKit reported invalid content geometry".to_owned());
    }
    let budget_scale = (max_pixels as f64 / native_pixels).sqrt().min(1.0);
    let width = (native_width * budget_scale).floor().max(1.0) as u32;
    let height = (native_height * budget_scale).floor().max(1.0) as u32;
    if u64::from(width) * u64::from(height) > max_pixels {
        return Err("Native capture dimensions exceed the transport pixel budget".to_owned());
    }
    Ok((width, height))
}

fn valid_positive(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}

/// Reads the ScreenCaptureKit status attachment while compensating for a
/// screencapturekit 8.0.1 bridge defect. That release casts Apple's NSNumber
/// attachment directly to the Swift `SCFrameStatus` enum, which returns
/// `None` for valid frames. Reading the raw CFNumber preserves the strict
/// Complete-only capture contract instead of treating an unknown status as
/// content.
fn frame_status(sample_buffer: &CMSampleBuffer) -> Result<SCFrameStatus, String> {
    if let Some(status) = sample_buffer.frame_status() {
        return Ok(status);
    }
    raw_frame_status(sample_buffer)
}

fn frame_status_is_complete(status: SCFrameStatus) -> bool {
    status == SCFrameStatus::Complete
}

fn raw_frame_status(sample_buffer: &CMSampleBuffer) -> Result<SCFrameStatus, String> {
    // SAFETY: `sample_buffer` owns a valid CMSampleBufferRef for this call.
    // CoreMedia owns the returned attachments and their nested objects; they
    // remain borrowed only while `sample_buffer` is alive. Every CF object is
    // type-checked before invoking its type-specific accessor.
    unsafe {
        let attachments =
            CMSampleBufferGetSampleAttachmentsArray(sample_buffer.as_ptr(), Boolean::from(false));
        if attachments.is_null() || CFArrayGetCount(attachments) <= 0 {
            return Err("the sample has no attachment dictionary".to_owned());
        }

        let first_attachment = CFArrayGetValueAtIndex(attachments, 0);
        if first_attachment.is_null() || CFGetTypeID(first_attachment) != CFDictionaryGetTypeID() {
            return Err("the first sample attachment is not a CFDictionary".to_owned());
        }
        let dictionary = first_attachment as CFDictionaryRef;
        let key = CFString::new(FRAME_STATUS_ATTACHMENT_KEY);
        let value = CFDictionaryGetValue(dictionary, key.as_CFTypeRef().cast());
        if value.is_null() {
            return Err(format!(
                "attachment {FRAME_STATUS_ATTACHMENT_KEY} is missing"
            ));
        }
        if CFGetTypeID(value) != CFNumberGetTypeID() {
            return Err(format!(
                "attachment {FRAME_STATUS_ATTACHMENT_KEY} is not a CFNumber"
            ));
        }

        let mut raw_status = 0_i32;
        if !CFNumberGetValue(
            value as CFNumberRef,
            kCFNumberSInt32Type,
            std::ptr::addr_of_mut!(raw_status).cast(),
        ) {
            return Err(format!(
                "attachment {FRAME_STATUS_ATTACHMENT_KEY} could not be converted to i32"
            ));
        }
        SCFrameStatus::from_raw(raw_status).ok_or_else(|| {
            format!("attachment {FRAME_STATUS_ATTACHMENT_KEY} has unknown value {raw_status}")
        })
    }
}

fn no_window(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_NO_WINDOW", message)
}

fn capture_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_CAPTURE_FAILED", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_geometry(width: u32, height: u32) -> CaptureGeometry {
        CaptureGeometry {
            x: 10,
            y: 20,
            width,
            height,
        }
    }

    fn test_dimensions(width: u32, height: u32) -> OutputDimensions {
        OutputDimensions { width, height }
    }

    #[test]
    fn bgra_conversion_is_channel_correct_and_stride_safe() {
        let bytes = [
            10, 20, 30, 40, 50, 60, 70, 80, 0xAA, 0xBB, 0xCC, 0xDD, 90, 100, 110, 120, 130, 140,
            150, 160, 0x11, 0x22, 0x33, 0x44,
        ];
        let image = bgra_to_rgba(2, 2, 12, &bytes).expect("valid padded BGRA frame");

        assert_eq!(image.dimensions(), (2, 2));
        assert_eq!(image.get_pixel(0, 0).0, [30, 20, 10, 40]);
        assert_eq!(image.get_pixel(1, 0).0, [70, 60, 50, 80]);
        assert_eq!(image.get_pixel(0, 1).0, [110, 100, 90, 120]);
        assert_eq!(image.get_pixel(1, 1).0, [150, 140, 130, 160]);
    }

    #[test]
    fn bgra_conversion_rejects_short_stride_and_truncated_buffer() {
        assert!(bgra_to_rgba(2, 1, 7, &[0; 8]).is_err());
        assert!(bgra_to_rgba(2, 2, 8, &[0; 15]).is_err());
    }

    #[test]
    fn frame_status_gate_is_complete_only() {
        assert!(frame_status_is_complete(SCFrameStatus::Complete));
        assert!(!frame_status_is_complete(SCFrameStatus::Started));
        assert!(!frame_status_is_complete(SCFrameStatus::Idle));
        assert!(!frame_status_is_complete(SCFrameStatus::Blank));
        assert!(!frame_status_is_complete(SCFrameStatus::Suspended));
        assert!(!frame_status_is_complete(SCFrameStatus::Stopped));
        assert!(SCFrameStatus::from_raw(-1).is_none());
        assert!(SCFrameStatus::from_raw(6).is_none());
    }

    #[test]
    fn capture_sizing_uses_retina_scale_and_transport_budget() {
        assert_eq!(
            capture_dimensions(400.0, 300.0, 2.0, 400, 300, 1_000_000)
                .expect("small Retina window"),
            (800, 600)
        );

        let dimensions = capture_dimensions(720.0, 492.0, 2.0, 720, 492, 1_000_000)
            .expect("budgeted Retina window");
        assert_eq!(dimensions, (1_209, 826));
        assert!(u64::from(dimensions.0) * u64::from(dimensions.1) <= 1_000_000);
    }

    #[test]
    fn capture_sizing_has_macos_13_geometry_fallback_and_rejects_empty_budget() {
        assert_eq!(
            capture_dimensions(0.0, 0.0, 0.0, 720, 492, 1_000_000).expect("macOS 13 fallback"),
            (720, 492)
        );
        assert_eq!(
            capture_dimensions(f64::NAN, f64::INFINITY, f64::NAN, 640, 480, 1_000_000)
                .expect("invalid modern geometry falls back"),
            (640, 480)
        );
        assert_eq!(
            capture_dimensions(1_200.0, 0.0, 1.0, 640, 480, 1_000_000)
                .expect("partial modern geometry falls back as a pair"),
            (640, 480)
        );
        assert!(capture_dimensions(100.0, 100.0, 2.0, 100, 100, 0).is_err());
    }

    #[test]
    fn stale_filter_geometry_falls_back_to_the_current_exact_window_aspect() {
        assert_eq!(
            geometry_aligned_content_size(720.0, 492.0, 720, 492),
            (720.0, 492.0)
        );
        assert_eq!(
            geometry_aligned_content_size(720.0, 492.0, 820, 552),
            (820.0, 552.0)
        );
        assert_eq!(
            geometry_aligned_content_size(720.0, 492.0, 1_440, 984),
            (1_440.0, 984.0),
            "a stale proportional content rect must not pin old resolution"
        );
        assert_eq!(
            geometry_aligned_content_size(0.0, f64::NAN, 820, 552),
            (820.0, 552.0)
        );
    }

    #[test]
    fn capture_state_is_latest_frame_wins_with_monotonic_sequences() {
        let geometry = test_geometry(400, 200);
        let state = CaptureState::new(test_dimensions(2, 1), geometry);
        let first = state
            .frame_candidate(None, test_dimensions(2, 1))
            .expect("initial frame candidate");
        state.store_frame(first, RgbaImage::new(2, 1), Instant::now());
        let second = state
            .frame_candidate(Some(2), test_dimensions(2, 1))
            .expect("newer initial frame candidate");
        state.store_frame(second, RgbaImage::new(2, 1), Instant::now());

        assert_eq!(state.dropped_frames(), 1);
        let frame = state
            .take_latest_after(0)
            .expect("healthy capture state")
            .expect("latest frame");
        assert_eq!(frame.source_sequence, 2);
        assert_eq!(frame.image.dimensions(), (2, 1));
        assert_eq!(frame.geometry, geometry);
        assert!(
            state
                .take_latest_after(frame.source_sequence)
                .expect("healthy capture state")
                .is_none()
        );

        let third = state
            .frame_candidate(Some(3), test_dimensions(2, 1))
            .expect("third initial frame candidate");
        state.store_frame(third, RgbaImage::new(2, 1), Instant::now());
        let next = state
            .take_latest_after(frame.source_sequence)
            .expect("healthy capture state")
            .expect("new frame");
        assert_eq!(next.source_sequence, 3);
        assert_eq!(next.image.dimensions(), (2, 1));
    }

    #[test]
    fn geometry_epoch_rejects_pre_update_wrong_size_and_in_flight_frames() {
        let old_geometry = test_geometry(720, 492);
        let new_geometry = test_geometry(820, 552);
        let old_dimensions = test_dimensions(1_209, 826);
        let new_dimensions = test_dimensions(1_218, 820);
        let state = CaptureState::new(old_dimensions, old_geometry);

        let in_flight = state
            .frame_candidate(Some(90), old_dimensions)
            .expect("old epoch candidate starts before the update");
        let epoch = state.begin_reconfiguration().expect("new geometry epoch");
        assert!(
            state.frame_candidate(Some(101), new_dimensions).is_none(),
            "callbacks stay paused while update_configuration is in flight"
        );
        state
            .finish_reconfiguration_at(epoch, 100, new_dimensions, new_geometry, Instant::now())
            .expect("configuration update completion");

        state.store_frame(in_flight, RgbaImage::new(1_209, 826), Instant::now());
        assert!(state.take_latest_after(0).unwrap().is_none());
        assert!(
            state.frame_candidate(Some(99), new_dimensions).is_none(),
            "a queued pre-boundary sample cannot enter the new geometry epoch"
        );
        assert!(
            state.frame_candidate(Some(101), old_dimensions).is_none(),
            "the previous configured dimensions cannot enter the new epoch"
        );

        let settled = state
            .frame_candidate(Some(101), new_dimensions)
            .expect("post-update exact-size candidate");
        state.store_frame(settled, RgbaImage::new(1_218, 820), Instant::now());
        let frame = state
            .take_latest_after(0)
            .expect("healthy reconfigured capture")
            .expect("settled frame");
        assert_eq!(frame.source_sequence, 1);
        assert_eq!(frame.geometry, new_geometry);
        assert_eq!(frame.image.dimensions(), (1_218, 820));
        assert!(state.dropped_frames() >= 3);
    }

    #[test]
    fn post_update_host_time_boundary_protects_same_output_dimensions_after_resize() {
        let output_dimensions = test_dimensions(1_000, 500);
        let state = CaptureState::new(output_dimensions, test_geometry(1_000, 500));
        let epoch = state.begin_reconfiguration().expect("new geometry epoch");
        state
            .finish_reconfiguration_at(
                epoch,
                500,
                output_dimensions,
                test_geometry(2_000, 1_000),
                Instant::now(),
            )
            .expect("same-aspect update");

        assert!(
            state
                .frame_candidate(Some(450), output_dimensions)
                .is_none(),
            "a sample displayed during update_configuration stays pre-epoch"
        );
        assert!(
            state
                .frame_candidate(Some(500), output_dimensions)
                .is_none()
        );
        let post_update = state
            .frame_candidate(Some(501), output_dimensions)
            .expect("strictly post-update frame");
        state.store_frame(post_update, RgbaImage::new(1_000, 500), Instant::now());
        assert_eq!(
            state
                .take_latest_after(0)
                .unwrap()
                .expect("geometry-bound frame")
                .geometry,
            test_geometry(2_000, 1_000)
        );
    }

    #[test]
    fn reconfiguration_without_a_geometry_bound_frame_fails_closed() {
        let dimensions = test_dimensions(2, 1);
        let geometry = test_geometry(400, 200);
        let state = CaptureState::new(dimensions, geometry);
        let epoch = state.begin_reconfiguration().expect("new geometry epoch");
        let expired = Instant::now()
            .checked_sub(RECONFIGURED_FRAME_TIMEOUT + Duration::from_millis(1))
            .expect("test instant");
        state
            .finish_reconfiguration_at(epoch, 10, dimensions, geometry, expired)
            .expect("configuration completion");

        let error = state
            .take_latest_after(0)
            .expect_err("missing settled frame must fail closed");
        assert_eq!(error.code, "COMPUTER_CAPTURE_FAILED");
        assert!(error.message.contains("geometry-bound frame"));
    }

    #[test]
    fn terminal_capture_error_discards_frames_and_fails_closed() {
        let state = CaptureState::new(test_dimensions(1, 1), test_geometry(1, 1));
        let candidate = state
            .frame_candidate(None, test_dimensions(1, 1))
            .expect("initial frame candidate");
        state.store_frame(candidate, RgbaImage::new(1, 1), Instant::now());
        state.fail(capture_error("permission revoked"));

        let error = state
            .take_latest_after(0)
            .expect_err("terminal errors must stop frame delivery");
        assert_eq!(error.code, "COMPUTER_CAPTURE_FAILED");
        assert!(error.message.contains("permission revoked"));
    }
}
