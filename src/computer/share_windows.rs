//! Persistent, exact-window Windows Graphics Capture sharing.
//!
//! A dedicated MTA owner thread creates the requested HWND's capture item and
//! a WinRT `Direct3D11CaptureFramePool::CreateFreeThreaded` pool. Free-threaded
//! callbacks publish into a capacity-one RGBA slot. There is deliberately no
//! monitor capture, title lookup, window fallback, global cursor capture, or
//! request to suppress Windows' normal capture border.

use std::ffi::c_void;
use std::fmt::{self, Formatter};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::slice;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use image::RgbaImage;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, RECT, S_FALSE};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsIconic, IsWindow};
use windows::core::{IInspectable, Interface, Ref, factory};

use super::{ComputerError, WindowDescriptor};

const MIN_SHARE_FPS: u64 = 1;
const MAX_SHARE_FPS: u64 = 10;
const BYTES_PER_PIXEL: u32 = 4;
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(4);
const OWNER_START_TIMEOUT: Duration = Duration::from_secs(4);
const OWNER_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const CALLBACK_DRAIN_TIMEOUT: Duration = Duration::from_millis(1_500);
// The helper has a separate 12-second hard command watchdog. Keep startup and
// any startup-only rollback inside a smaller total budget so the worker can
// report a specific capture failure before that outer watchdog revokes it.
const STARTUP_TOTAL_TIMEOUT: Duration = Duration::from_secs(10);
const STARTUP_ROLLBACK_RESERVE: Duration = OWNER_STOP_TIMEOUT;
const STARTUP_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_WGC_SOURCE_PIXELS: u64 = 16_777_216;
const MAX_WGC_SOURCE_DIMENSION: u32 = 8_192;
const MAX_WGC_OUTPUT_PIXELS: u64 = 1_000_000;
const MAX_WGC_ROW_PITCH: u32 = MAX_WGC_SOURCE_DIMENSION * BYTES_PER_PIXEL * 2;
const MAX_WGC_MAPPED_BYTES: u64 = MAX_WGC_SOURCE_PIXELS * BYTES_PER_PIXEL as u64 * 2;
const HUNDRED_NANOSECONDS_PER_SECOND: i128 = 10_000_000;
const CAPTURE_BUFFER_COUNT: i32 = 1;
const CAPTURE_PIXEL_FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
const FATAL_CAPTURE_STOP_CODE: &str = "COMPUTER_CAPTURE_STOP_FATAL";
const NATIVE_SHARE_METADATA: NativeShareMetadata = NativeShareMetadata {
    backend: "windows-graphics-capture/create-free-threaded",
    native_stream: true,
    system_indicator: true,
    selection_mode: "programmatic-exact-window",
};
static FATAL_CAPTURE_STOP: Mutex<Option<ComputerError>> = Mutex::new(None);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn QueryPerformanceCounter(value: *mut i64) -> i32;
    fn QueryPerformanceFrequency(value: *mut i64) -> i32;
}

#[derive(Debug)]
pub(crate) struct NativeShareFrame {
    pub(crate) image: RgbaImage,
    pub(crate) source_sequence: u64,
    pub(crate) captured_at: Instant,
    pub(crate) source_x: i32,
    pub(crate) source_y: i32,
    pub(crate) source_width: u32,
    pub(crate) source_height: u32,
}

pub(crate) fn bind_frame_geometry(frame: &NativeShareFrame, target: &mut WindowDescriptor) {
    target.x = frame.source_x;
    target.y = frame.source_y;
    target.width = frame.source_width;
    target.height = frame.source_height;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceGeometry {
    x: i32,
    y: i32,
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

    fn lock_for_handler(&self) -> Result<MutexGuard<'_, FrameSlot>, String> {
        self.slot.lock().map_err(|_| {
            format!(
                "native capture state for HWND {} (PID {}) was poisoned",
                self.target_id, self.target_pid
            )
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

    fn publish(
        &self,
        image: RgbaImage,
        captured_at: Instant,
        geometry: SourceGeometry,
    ) -> Result<(), String> {
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
            return Err(message.to_string());
        };
        slot.source_sequence = source_sequence;
        let replaced = slot
            .latest
            .replace(NativeShareFrame {
                image,
                source_sequence,
                captured_at,
                source_x: geometry.x,
                source_y: geometry.y,
                source_width: geometry.width,
                source_height: geometry.height,
            })
            .is_some();
        drop(slot);
        self.changed.notify_all();

        if replaced {
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    fn mark_closed(&self) -> Result<(), String> {
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

#[derive(Debug, Clone)]
struct TargetBinding {
    id: String,
    pid: u32,
    hwnd_value: usize,
    expected_geometry: SourceGeometry,
}

impl TargetBinding {
    fn hwnd(&self) -> HWND {
        HWND(self.hwnd_value as *mut c_void)
    }
}

#[derive(Debug, Default)]
struct CallbackGateState {
    accepting: bool,
    in_flight: usize,
}

#[derive(Debug)]
struct CallbackGate {
    state: Mutex<CallbackGateState>,
    drained: Condvar,
}

impl CallbackGate {
    fn new() -> Self {
        Self {
            state: Mutex::new(CallbackGateState {
                accepting: true,
                in_flight: 0,
            }),
            drained: Condvar::new(),
        }
    }

    fn enter(self: &Arc<Self>) -> Result<Option<CallbackAdmission>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "WGC callback admission state was poisoned".to_string())?;
        if !state.accepting {
            return Ok(None);
        }
        state.in_flight = state
            .in_flight
            .checked_add(1)
            .ok_or_else(|| "WGC in-flight callback count overflowed".to_string())?;
        drop(state);
        Ok(Some(CallbackAdmission {
            gate: Arc::clone(self),
        }))
    }

    fn begin_shutdown(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "WGC callback admission state was poisoned during shutdown".to_string())?;
        state.accepting = false;
        Ok(())
    }

    fn wait_drained(&self, timeout: Duration) -> Result<(), String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "WGC callback state was poisoned while draining".to_string())?;
        let (state, _timed) = self
            .drained
            .wait_timeout_while(state, timeout, |state| state.in_flight != 0)
            .map_err(|_| "WGC callback state was poisoned while draining".to_string())?;
        if state.in_flight == 0 {
            Ok(())
        } else {
            Err(format!(
                "{} WGC callback(s) remained in flight after {} ms",
                state.in_flight,
                timeout.as_millis()
            ))
        }
    }
}

#[derive(Debug)]
struct CallbackAdmission {
    gate: Arc<CallbackGate>,
}

impl Drop for CallbackAdmission {
    fn drop(&mut self) {
        let Ok(mut state) = self.gate.state.lock() else {
            self.gate.drained.notify_all();
            return;
        };
        state.in_flight = state.in_flight.saturating_sub(1);
        let drained = state.in_flight == 0;
        drop(state);
        if drained {
            self.gate.drained.notify_all();
        }
    }
}

#[derive(Debug)]
struct FrameProcessor {
    pool_size: SizeInt32,
    last_published_at: Option<Instant>,
}

struct FrameCallbackContext {
    state: Arc<SharedCaptureState>,
    gate: Arc<CallbackGate>,
    target: TargetBinding,
    frame_pool: Direct3D11CaptureFramePool,
    direct3d_device: Mutex<SendDirect3DDevice>,
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    minimum_interval: Duration,
    processor: Mutex<FrameProcessor>,
    stop_sender: Sender<OwnerCommand>,
}

/// `CreateDirect3D11DeviceFromDXGIDevice` returns the agile WinRT wrapper for
/// a multithread-protected D3D11 device. The windows crate does not encode that
/// runtime agility on `IDirect3DDevice`, so moving the wrapper is made explicit
/// and every use remains serialized by this field's `Mutex`.
struct SendDirect3DDevice(IDirect3DDevice);

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for SendDirect3DDevice {}

impl fmt::Debug for FrameCallbackContext {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrameCallbackContext")
            .field("target", &self.target)
            .field("minimum_interval", &self.minimum_interval)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy)]
enum OwnerCommand {
    Stop,
}

struct CaptureRuntime {
    item: GraphicsCaptureItem,
    frame_pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    frame_token: Option<i64>,
    closed_token: Option<i64>,
    gate: Arc<CallbackGate>,
    frame_context: Arc<FrameCallbackContext>,
    closed: bool,
}

impl CaptureRuntime {
    fn shutdown(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let mut failures = Vec::new();

        if let Err(error) = self.gate.begin_shutdown() {
            return Err(self.abandon_unconfirmed_shutdown(error));
        }
        if let Some(token) = self.frame_token.take()
            && let Err(error) = self.frame_pool.RemoveFrameArrived(token)
        {
            failures.push(format!("RemoveFrameArrived failed: {error}"));
        }
        if let Some(token) = self.closed_token.take()
            && let Err(error) = self.item.RemoveClosed(token)
        {
            failures.push(format!("RemoveClosed failed: {error}"));
        }
        if let Err(error) = self.gate.wait_drained(CALLBACK_DRAIN_TIMEOUT) {
            failures.push(error);
            return Err(self.abandon_unconfirmed_shutdown(failures.join("; ")));
        }

        // CreateFreeThreaded callbacks may execute on arbitrary worker threads.
        // Do not mutate or close any shared capture resource until every callback
        // admitted before the gate closed has returned.
        if let Err(error) = self.session.Close() {
            failures.push(format!("GraphicsCaptureSession::Close failed: {error}"));
        }
        if let Err(error) = self.frame_pool.Close() {
            failures.push(format!("Direct3D11CaptureFramePool::Close failed: {error}"));
        }
        match self.frame_context.direct3d_device.lock() {
            Ok(device) => {
                if let Err(error) = device.0.Close() {
                    failures.push(format!("WGC Direct3D device Close failed: {error}"));
                }
            }
            Err(_) => {
                failures.push("WGC Direct3D device state was poisoned during shutdown".to_string())
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn abandon_unconfirmed_shutdown(&self, detail: String) -> String {
        // The helper worker is disposable. If callback admission or draining
        // cannot be proved, keep every COM/D3D owner alive until that process is
        // terminated instead of releasing resources underneath an in-flight
        // callback. Leaking one reference is deliberate and bounded by the
        // worker supervisor's hard lifetime.
        std::mem::forget(self.item.clone());
        std::mem::forget(self.frame_pool.clone());
        std::mem::forget(self.session.clone());
        std::mem::forget(Arc::clone(&self.gate));
        std::mem::forget(Arc::clone(&self.frame_context));

        let message = format!(
            "WGC shutdown was not proven safe; capture resources were retained for worker termination: {detail}"
        );
        let _ = fatal_stop_error(message.clone());
        message
    }
}

impl Drop for CaptureRuntime {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            let _ = fatal_stop_error(format!(
                "WGC runtime dropped without confirmed cleanup: {error}"
            ));
        }
    }
}

struct RoApartment;

impl RoApartment {
    fn initialize() -> Result<Self, ComputerError> {
        match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            Ok(()) => Ok(Self),
            Err(error) if error.code() == S_FALSE => Ok(Self),
            Err(error) => Err(capture_error(format!(
                "Failed to initialize the WGC owner MTA: {error}"
            ))),
        }
    }
}

impl Drop for RoApartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

pub(crate) struct NativeShareCapture {
    command_sender: Option<Sender<OwnerCommand>>,
    owner: Option<JoinHandle<Result<(), ComputerError>>>,
    state: Arc<SharedCaptureState>,
    window_id: String,
    fps: u64,
}

impl NativeShareCapture {
    pub(crate) fn start(target: &WindowDescriptor, fps: u64) -> Result<Self, ComputerError> {
        let startup_deadline = Instant::now() + STARTUP_TOTAL_TIMEOUT;
        let readiness_deadline = startup_deadline - STARTUP_ROLLBACK_RESERVE;
        let minimum_interval = share_interval(fps)?;
        let target_binding = validate_descriptor_geometry(target)?;
        let state = Arc::new(SharedCaptureState::new(target.id.clone(), target.pid));
        let (command_sender, command_receiver) = mpsc::channel();
        let owner_command_sender = command_sender.clone();
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let owner_state = Arc::clone(&state);
        let owner_target = target_binding.clone();
        let owner = thread::Builder::new()
            .name("lbb-wgc-owner".to_string())
            .spawn(move || {
                capture_owner_main(
                    owner_target,
                    owner_state,
                    minimum_interval,
                    owner_command_sender,
                    command_receiver,
                    startup_sender,
                )
            })
            .map_err(|error| {
                capture_error(format!("Failed to start the WGC owner thread: {error}"))
            })?;

        let mut capture = Self {
            command_sender: Some(command_sender),
            owner: Some(owner),
            state,
            window_id: target.id.clone(),
            fps,
        };

        let Some(owner_start_timeout) =
            bounded_wait_timeout(readiness_deadline, OWNER_START_TIMEOUT)
        else {
            stop_startup_capture(
                &mut capture,
                "owner readiness budget exhaustion",
                startup_deadline,
            )?;
            return Err(startup_budget_error(target, "WGC owner readiness"));
        };
        match startup_receiver.recv_timeout(owner_start_timeout) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                capture.confirm_reported_startup_failure_before(startup_deadline, &error)?;
                return Err(error);
            }
            Err(error) => {
                let message = format!(
                    "WGC owner did not confirm startup for HWND {} (PID {}) within {} ms: {error}",
                    target.id,
                    target.pid,
                    owner_start_timeout.as_millis()
                );
                if let Some(sender) = capture.command_sender.take() {
                    let _ = sender.send(OwnerCommand::Stop);
                }
                // A setup call may be blocked in a graphics driver. Do not
                // join an unconfirmed owner here; the disposable helper worker
                // must terminate before another capture is trusted.
                capture.owner.take();
                return Err(fatal_stop_error(message));
            }
        }

        let Some(first_frame_timeout) =
            bounded_wait_timeout(readiness_deadline, FIRST_FRAME_TIMEOUT)
        else {
            stop_startup_capture(
                &mut capture,
                "first-frame readiness budget exhaustion",
                startup_deadline,
            )?;
            return Err(startup_budget_error(target, "first-frame readiness"));
        };
        if let Err(error) = capture.state.wait_for_first_frame(first_frame_timeout) {
            stop_startup_capture(
                &mut capture,
                "first-frame readiness failure",
                startup_deadline,
            )?;
            return Err(error);
        }

        Ok(capture)
    }

    /// WGC owns resize handling inside its frame-pool owner and binds every
    /// delivered frame to capture-time DWM geometry. Keep the common controller
    /// hook explicit so macOS can update SCStream before either backend's next
    /// native frame is accepted.
    pub(crate) fn prepare_for_target(
        &mut self,
        _target: &WindowDescriptor,
    ) -> Result<bool, ComputerError> {
        Ok(false)
    }

    pub(crate) fn latest_after(
        &self,
        last_source_sequence: u64,
    ) -> Result<Option<NativeShareFrame>, ComputerError> {
        let latest = self.state.take_after(last_source_sequence)?;
        if latest.is_none() && self.owner.as_ref().is_some_and(JoinHandle::is_finished) {
            return Err(capture_error(format!(
                "The WGC capture thread for HWND {} (PID {}) ended unexpectedly",
                self.state.target_id, self.state.target_pid
            )));
        }
        Ok(latest)
    }

    pub(crate) fn stop(&mut self) -> Result<(), ComputerError> {
        self.stop_before(Instant::now() + OWNER_STOP_TIMEOUT)
    }

    fn stop_before(&mut self, deadline: Instant) -> Result<(), ComputerError> {
        let Some(owner) = self.owner.take() else {
            return Ok(());
        };
        if let Some(sender) = self.command_sender.take() {
            let _ = sender.send(OwnerCommand::Stop);
        }
        if !wait_for_owner_exit(&owner, deadline) {
            return Err(fatal_stop_error(format!(
                "The WGC owner did not confirm shutdown before the internal deadline for HWND {} (PID {})",
                self.state.target_id, self.state.target_pid
            )));
        }
        let owner_result = owner.join().map_err(|_| {
            fatal_stop_error(format!(
                "The WGC owner thread panicked while stopping HWND {} (PID {})",
                self.state.target_id, self.state.target_pid
            ))
        })?;
        owner_result?;
        self.state.clear_latest()
    }

    fn confirm_reported_startup_failure_before(
        &mut self,
        deadline: Instant,
        reported_error: &ComputerError,
    ) -> Result<(), ComputerError> {
        self.command_sender.take();
        let Some(owner) = self.owner.take() else {
            let message = format!(
                "The WGC owner handle disappeared after reporting startup failure for HWND {} (PID {})",
                self.state.target_id, self.state.target_pid
            );
            self.state.mark_failed(message.clone());
            return Err(fatal_stop_error(message));
        };
        if !wait_for_owner_exit(&owner, deadline) {
            let message = format!(
                "The WGC owner did not confirm exit after reporting startup failure before the internal deadline for HWND {} (PID {})",
                self.state.target_id, self.state.target_pid
            );
            self.state.mark_failed(message.clone());
            return Err(fatal_stop_error(message));
        }
        let owner_result = owner.join().map_err(|_| {
            let message = format!(
                "The WGC owner thread panicked after reporting startup failure for HWND {} (PID {})",
                self.state.target_id, self.state.target_pid
            );
            self.state.mark_failed(message.clone());
            fatal_stop_error(message)
        })?;
        match owner_result {
            Err(owner_error)
                if owner_error.code == reported_error.code
                    && owner_error.message == reported_error.message =>
            {
                self.state.clear_latest().map_err(|error| {
                    let message = format!(
                        "The WGC owner confirmed startup failure for HWND {} (PID {}), but capture-state rollback was unconfirmed: {error}",
                        self.state.target_id, self.state.target_pid
                    );
                    self.state.mark_failed(message.clone());
                    fatal_stop_error(message)
                })
            }
            Err(owner_error) => {
                let message = format!(
                    "The WGC owner reported startup error {} ({}) for HWND {} (PID {}) but exited with different error {} ({})",
                    reported_error.code,
                    reported_error.message,
                    self.state.target_id,
                    self.state.target_pid,
                    owner_error.code,
                    owner_error.message
                );
                self.state.mark_failed(message.clone());
                Err(fatal_stop_error(message))
            }
            Ok(()) => {
                let message = format!(
                    "The WGC owner reported startup failure but exited successfully for HWND {} (PID {})",
                    self.state.target_id, self.state.target_pid
                );
                self.state.mark_failed(message.clone());
                Err(fatal_stop_error(message))
            }
        }
    }

    pub(crate) const fn metadata(&self) -> NativeShareMetadata {
        NATIVE_SHARE_METADATA
    }

    pub(crate) fn dropped_frames(&self) -> u64 {
        self.state.dropped_frames()
    }
}

fn stop_startup_capture(
    capture: &mut NativeShareCapture,
    rollback_reason: &str,
    deadline: Instant,
) -> Result<(), ComputerError> {
    if let Err(error) = capture.stop_before(deadline) {
        let message = format!(
            "Could not confirm WGC startup rollback after {rollback_reason} for HWND {} (PID {}): {error}",
            capture.state.target_id, capture.state.target_pid
        );
        capture.state.mark_failed(message.clone());
        return Err(fatal_stop_error(message));
    }
    Ok(())
}

fn bounded_wait_timeout(deadline: Instant, maximum: Duration) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .map(|remaining| remaining.min(maximum))
        .filter(|remaining| !remaining.is_zero())
}

fn wait_for_owner_exit(owner: &JoinHandle<Result<(), ComputerError>>, deadline: Instant) -> bool {
    loop {
        if owner.is_finished() {
            return true;
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return false;
        };
        if remaining.is_zero() {
            return false;
        }
        thread::sleep(remaining.min(STARTUP_JOIN_POLL_INTERVAL));
    }
}

fn startup_budget_error(target: &WindowDescriptor, phase: &str) -> ComputerError {
    capture_error(format!(
        "Windows Graphics Capture exhausted its {} ms internal startup budget during {phase} for HWND {} (PID {})",
        STARTUP_TOTAL_TIMEOUT.as_millis(),
        target.id,
        target.pid
    ))
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

pub(crate) fn capture_one(target: &WindowDescriptor) -> Result<NativeShareFrame, ComputerError> {
    let mut capture = NativeShareCapture::start(target, MAX_SHARE_FPS)?;
    let frame_result = capture.state.take_after(0).and_then(|frame| {
        frame.ok_or_else(|| capture_error("WGC first-frame readiness lost its buffered frame"))
    });
    let stop_result = capture.stop();
    stop_result?;
    frame_result
}

fn capture_owner_main(
    target: TargetBinding,
    state: Arc<SharedCaptureState>,
    minimum_interval: Duration,
    command_sender: Sender<OwnerCommand>,
    command_receiver: Receiver<OwnerCommand>,
    startup_sender: mpsc::SyncSender<Result<(), ComputerError>>,
) -> Result<(), ComputerError> {
    let _apartment = match RoApartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = startup_sender.send(Err(error.clone()));
            return Err(error);
        }
    };
    let mut runtime = match create_capture_runtime(
        target,
        Arc::clone(&state),
        minimum_interval,
        command_sender,
    ) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = startup_sender.send(Err(error.clone()));
            return Err(error);
        }
    };
    if startup_sender.send(Ok(())).is_err() {
        let cleanup = runtime.shutdown().map_err(|error| {
            fatal_stop_error(format!("WGC startup receiver disappeared: {error}"))
        });
        return cleanup;
    }

    let _ = command_receiver.recv();
    runtime.shutdown().map_err(|error| {
        let message = format!(
            "Could not confirm WGC shutdown for HWND {} (PID {}): {error}",
            state.target_id, state.target_pid
        );
        state.mark_failed(message.clone());
        fatal_stop_error(message)
    })
}

fn create_capture_runtime(
    target: TargetBinding,
    state: Arc<SharedCaptureState>,
    minimum_interval: Duration,
    stop_sender: Sender<OwnerCommand>,
) -> Result<CaptureRuntime, ComputerError> {
    validate_target_binding_geometry(&target)?;
    if !GraphicsCaptureSession::IsSupported().map_err(windows_capture_error)? {
        return Err(capture_error(
            "Windows Graphics Capture is not supported on this Windows build",
        ));
    }

    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(windows_capture_error)?;
    let item: GraphicsCaptureItem =
        unsafe { interop.CreateForWindow(target.hwnd()) }.map_err(windows_capture_error)?;
    exact_window_pair(&target)?;
    let item_size = item.Size().map_err(windows_capture_error)?;
    validate_winrt_size(item_size, "initial GraphicsCaptureItem::Size").map_err(capture_error)?;

    let (d3d_device, d3d_context, direct3d_device) = create_capture_device()?;
    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &direct3d_device,
        CAPTURE_PIXEL_FORMAT,
        CAPTURE_BUFFER_COUNT,
        item_size,
    )
    .map_err(windows_capture_error)?;
    let session = match frame_pool.CreateCaptureSession(&item) {
        Ok(session) => session,
        Err(error) => {
            let close = frame_pool.Close();
            return Err(startup_failure("CreateCaptureSession", error, close.err()));
        }
    };
    let gate = Arc::new(CallbackGate::new());
    let frame_context = Arc::new(FrameCallbackContext {
        state: Arc::clone(&state),
        gate: Arc::clone(&gate),
        target: target.clone(),
        frame_pool: frame_pool.clone(),
        direct3d_device: Mutex::new(SendDirect3DDevice(direct3d_device)),
        d3d_device,
        d3d_context,
        minimum_interval,
        processor: Mutex::new(FrameProcessor {
            pool_size: item_size,
            last_published_at: None,
        }),
        stop_sender: stop_sender.clone(),
    });
    let mut runtime = CaptureRuntime {
        item,
        frame_pool,
        session,
        frame_token: None,
        closed_token: None,
        gate,
        frame_context: Arc::clone(&frame_context),
        closed: false,
    };

    let frame_handler = TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new({
        move |sender, _| {
            run_frame_callback(&frame_context, sender);
            Ok(())
        }
    });
    runtime.frame_token = match runtime.frame_pool.FrameArrived(&frame_handler) {
        Ok(token) => Some(token),
        Err(error) => return Err(startup_runtime_failure(&mut runtime, "FrameArrived", error)),
    };

    let closed_state = Arc::clone(&state);
    let closed_gate = Arc::clone(&runtime.gate);
    let closed_target = target.clone();
    let closed_stop_sender = stop_sender;
    let closed_handler =
        TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
            run_closed_callback(
                &closed_state,
                &closed_gate,
                &closed_target,
                &closed_stop_sender,
            );
            Ok(())
        });
    runtime.closed_token = match runtime.item.Closed(&closed_handler) {
        Ok(token) => Some(token),
        Err(error) => return Err(startup_runtime_failure(&mut runtime, "Closed", error)),
    };

    // Cursor suppression is mandatory. We intentionally never call
    // SetIsBorderRequired: Windows retains its default capture indication.
    if let Err(error) = runtime.session.SetIsCursorCaptureEnabled(false) {
        return Err(startup_runtime_failure(
            &mut runtime,
            "SetIsCursorCaptureEnabled(false)",
            error,
        ));
    }
    if let Err(error) = runtime.session.StartCapture() {
        return Err(startup_runtime_failure(&mut runtime, "StartCapture", error));
    }
    if let Err(error) = exact_window_pair(&target) {
        let cleanup_error = runtime.shutdown().err();
        return match cleanup_error {
            Some(cleanup_error) => Err(fatal_stop_error(format!(
                "WGC target revalidation failed ({error}) and startup rollback was unconfirmed ({cleanup_error})"
            ))),
            None => Err(error),
        };
    }
    Ok(runtime)
}

fn run_frame_callback(
    context: &Arc<FrameCallbackContext>,
    sender: Ref<'_, Direct3D11CaptureFramePool>,
) {
    let admission = match context.gate.enter() {
        Ok(Some(admission)) => admission,
        Ok(None) => return,
        Err(error) => {
            context.state.mark_failed(error);
            let _ = context.stop_sender.send(OwnerCommand::Stop);
            return;
        }
    };
    let result = catch_unwind(AssertUnwindSafe(|| process_frame_callback(context, sender)))
        .unwrap_or_else(|_| Err("WGC frame callback panicked".to_string()));
    if let Err(error) = result {
        context.state.mark_failed(error);
        let _ = context.stop_sender.send(OwnerCommand::Stop);
    }
    drop(admission);
}

fn process_frame_callback(
    context: &Arc<FrameCallbackContext>,
    sender: Ref<'_, Direct3D11CaptureFramePool>,
) -> Result<(), String> {
    let geometry_before = exact_window_geometry(&context.target).map_err(|error| error.message)?;
    let frame_pool = sender
        .as_ref()
        .ok_or_else(|| "WGC FrameArrived supplied a null frame pool".to_string())?;
    let mut frame = FrameLease::new(
        frame_pool
            .TryGetNextFrame()
            .map_err(|error| format!("TryGetNextFrame failed: {error}"))?,
    );
    let content_size = frame
        .get()
        .ContentSize()
        .map_err(|error| format!("Direct3D11CaptureFrame::ContentSize failed: {error}"))?;
    validate_winrt_size(content_size, "frame ContentSize")?;

    let mut processor = context
        .processor
        .lock()
        .map_err(|_| "WGC frame processor state was poisoned".to_string())?;
    if content_size != processor.pool_size {
        frame.close("resized WGC frame")?;
        exact_window_pair(&context.target).map_err(|error| error.message)?;
        // ContentSize was bounded above before this source-sized allocation.
        let direct3d_device = context
            .direct3d_device
            .lock()
            .map_err(|_| "WGC Direct3D device state was poisoned".to_string())?;
        context
            .frame_pool
            .Recreate(
                &direct3d_device.0,
                CAPTURE_PIXEL_FORMAT,
                CAPTURE_BUFFER_COUNT,
                content_size,
            )
            .map_err(|error| format!("bounded WGC frame-pool Recreate failed: {error}"))?;
        processor.pool_size = content_size;
        return Ok(());
    }

    let captured_at = compositor_captured_at(frame.get())?;
    if processor
        .last_published_at
        .is_some_and(|last| captured_at.saturating_duration_since(last) < context.minimum_interval)
    {
        frame.close("throttled WGC frame")?;
        return Ok(());
    }

    let image_result = copy_frame_to_image(
        frame.get(),
        &context.d3d_device,
        &context.d3d_context,
        content_size,
    );
    let close_result = frame.close("published WGC frame");
    let image = image_result?;
    close_result.map_err(|error| format!("WGC frame Close failed: {error}"))?;
    let geometry_after = exact_window_geometry(&context.target).map_err(|error| error.message)?;
    let (content_width, content_height) =
        validate_winrt_size(content_size, "frame ContentSize before publication")?;
    if geometry_before != geometry_after
        || geometry_after.width != content_width
        || geometry_after.height != content_height
    {
        // A move or resize can race an already-composed frame. Discard that
        // frame instead of pairing its pixels with geometry from another
        // window state; the capacity-one stream will supply a settled frame.
        return Ok(());
    }
    context.state.publish(image, captured_at, geometry_after)?;
    processor.last_published_at = Some(captured_at);
    Ok(())
}

struct FrameLease {
    frame: Direct3D11CaptureFrame,
    closed: bool,
}

impl FrameLease {
    const fn new(frame: Direct3D11CaptureFrame) -> Self {
        Self {
            frame,
            closed: false,
        }
    }

    const fn get(&self) -> &Direct3D11CaptureFrame {
        &self.frame
    }

    fn close(&mut self, context: &str) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.frame.Close().map_err(|error| {
            let message = format!("{context} Close was unconfirmed: {error}");
            let _ = fatal_stop_error(message.clone());
            message
        })
    }
}

impl Drop for FrameLease {
    fn drop(&mut self) {
        if let Err(error) = self.close("discarded WGC frame") {
            let _ = fatal_stop_error(error);
        }
    }
}

fn run_closed_callback(
    state: &Arc<SharedCaptureState>,
    gate: &Arc<CallbackGate>,
    target: &TargetBinding,
    stop_sender: &Sender<OwnerCommand>,
) {
    let admission = match gate.enter() {
        Ok(Some(admission)) => admission,
        Ok(None) => return,
        Err(error) => {
            state.mark_failed(error);
            let _ = stop_sender.send(OwnerCommand::Stop);
            return;
        }
    };
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        if let Err(error) = exact_window_pair(target)
            && error.code != "COMPUTER_NO_WINDOW"
        {
            return Err(error.message);
        }
        state.mark_closed()
    }))
    .unwrap_or_else(|_| Err("WGC target-closed callback panicked".to_string()));
    if let Err(error) = result {
        state.mark_failed(error);
    }
    let _ = stop_sender.send(OwnerCommand::Stop);
    drop(admission);
}

fn copy_frame_to_image(
    frame: &Direct3D11CaptureFrame,
    d3d_device: &ID3D11Device,
    d3d_context: &ID3D11DeviceContext,
    content_size: SizeInt32,
) -> Result<RgbaImage, String> {
    let (content_width, content_height) =
        validate_winrt_size(content_size, "frame ContentSize before texture access")?;
    let surface = frame
        .Surface()
        .map_err(|error| format!("WGC frame Surface failed: {error}"))?;
    let access = surface
        .cast::<IDirect3DDxgiInterfaceAccess>()
        .map_err(|error| format!("WGC surface DXGI cast failed: {error}"))?;
    let source_texture = unsafe { access.GetInterface::<ID3D11Texture2D>() }
        .map_err(|error| format!("WGC surface texture access failed: {error}"))?;
    let mut source_desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { source_texture.GetDesc(&mut source_desc) };
    validate_texture_desc(&source_desc, content_width, content_height)?;

    // The source descriptor is bounded and matched to ContentSize before this
    // staging allocation is allowed.
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: source_desc.Width,
        Height: source_desc.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: source_desc.Format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging = None;
    unsafe { d3d_device.CreateTexture2D(&staging_desc, None, Some(&mut staging)) }
        .map_err(|error| format!("bounded WGC staging texture allocation failed: {error}"))?;
    let staging = staging.ok_or_else(|| {
        "D3D11CreateTexture2D succeeded without returning a staging texture".to_string()
    })?;
    unsafe { d3d_context.CopyResource(&staging, &source_texture) };

    let mapped = MappedTexture::map(d3d_context, &staging)?;
    copy_bgra_rows_to_rgba(
        mapped.bytes(content_width, content_height)?,
        content_width,
        content_height,
        mapped.row_pitch(),
    )
}

struct MappedTexture<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
    mapped: D3D11_MAPPED_SUBRESOURCE,
}

impl<'a> MappedTexture<'a> {
    fn map(context: &'a ID3D11DeviceContext, texture: &'a ID3D11Texture2D) -> Result<Self, String> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { context.Map(texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
            .map_err(|error| format!("bounded WGC staging texture Map failed: {error}"))?;
        Ok(Self {
            context,
            texture,
            mapped,
        })
    }

    fn row_pitch(&self) -> u32 {
        self.mapped.RowPitch
    }

    fn bytes(&self, width: u32, height: u32) -> Result<&[u8], String> {
        if self.mapped.pData.is_null() {
            return Err("D3D11 Map returned a null staging pointer".to_string());
        }
        let length = bounded_mapped_byte_len(width, height, self.mapped.RowPitch)?;
        // SAFETY: D3D11 Map guarantees RowPitch bytes for each row of the
        // mapped subresource. Width, height, RowPitch, and their product were
        // all validated against project-owned hard limits above.
        Ok(unsafe { slice::from_raw_parts(self.mapped.pData.cast(), length) })
    }
}

fn bounded_mapped_byte_len(width: u32, height: u32, row_pitch: u32) -> Result<usize, String> {
    validate_source_dimensions(width, height)?;
    let packed_row = width
        .checked_mul(BYTES_PER_PIXEL)
        .ok_or_else(|| "mapped WGC packed row size overflowed".to_string())?;
    if row_pitch < packed_row || row_pitch > MAX_WGC_ROW_PITCH {
        return Err(format!(
            "D3D11 Map returned out-of-bounds RowPitch {row_pitch} for packed width {packed_row}"
        ));
    }
    let mapped_bytes = u64::from(height)
        .checked_mul(u64::from(row_pitch))
        .ok_or_else(|| "mapped WGC staging byte length overflowed".to_string())?;
    if mapped_bytes > MAX_WGC_MAPPED_BYTES {
        return Err(format!(
            "D3D11 Map returned {mapped_bytes} bytes, exceeding the bounded staging budget {MAX_WGC_MAPPED_BYTES}"
        ));
    }
    usize::try_from(mapped_bytes)
        .map_err(|_| "mapped WGC staging byte length does not fit memory indexing".to_string())
}

impl Drop for MappedTexture<'_> {
    fn drop(&mut self) {
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}

fn create_capture_device()
-> Result<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice), ComputerError> {
    let levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
    let mut device = None;
    let mut context = None;
    let mut selected_level = D3D_FEATURE_LEVEL::default();
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut selected_level),
            Some(&mut context),
        )
    }
    .map_err(windows_capture_error)?;
    if selected_level.0 < D3D_FEATURE_LEVEL_11_0.0 {
        return Err(capture_error(
            "The WGC D3D11 device did not provide feature level 11.0",
        ));
    }
    let device = device.ok_or_else(|| capture_error("D3D11CreateDevice returned no device"))?;
    let context =
        context.ok_or_else(|| capture_error("D3D11CreateDevice returned no immediate context"))?;
    let dxgi_device: IDXGIDevice = device.cast().map_err(windows_capture_error)?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }
        .map_err(windows_capture_error)?;
    let direct3d_device: IDirect3DDevice = inspectable.cast().map_err(windows_capture_error)?;
    Ok((device, context, direct3d_device))
}

fn validate_descriptor_geometry(target: &WindowDescriptor) -> Result<TargetBinding, ComputerError> {
    validate_source_dimensions(target.width, target.height).map_err(capture_error)?;
    if target.minimized {
        return Err(no_window_error(
            "The requested exact window is minimized and cannot be captured",
        ));
    }
    let hwnd_value = target.id.parse::<usize>().map_err(|_| {
        no_window_error(format!(
            "The exact target window ID {:?} is not a valid HWND",
            target.id
        ))
    })?;
    if hwnd_value == 0 {
        return Err(no_window_error("The exact target HWND is null"));
    }
    let binding = TargetBinding {
        id: target.id.clone(),
        pid: target.pid,
        hwnd_value,
        expected_geometry: SourceGeometry {
            x: target.x,
            y: target.y,
            width: target.width,
            height: target.height,
        },
    };
    Ok(binding)
}

fn validate_target_binding_geometry(target: &TargetBinding) -> Result<(), ComputerError> {
    let geometry = exact_window_geometry(target)?;
    if geometry != target.expected_geometry {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            format!(
                "The exact target HWND {} geometry changed from ({}, {}) {}x{} to ({}, {}) {}x{} before capture allocation",
                target.id,
                target.expected_geometry.x,
                target.expected_geometry.y,
                target.expected_geometry.width,
                target.expected_geometry.height,
                geometry.x,
                geometry.y,
                geometry.width,
                geometry.height
            ),
        ));
    }
    Ok(())
}

fn exact_window_pair(target: &TargetBinding) -> Result<(), ComputerError> {
    let hwnd = target.hwnd();
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err(no_window_error(format!(
            "The exact target HWND {} no longer exists",
            target.id
        )));
    }
    let mut actual_pid = 0_u32;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut actual_pid)) };
    if thread_id == 0 || actual_pid == 0 {
        return Err(no_window_error(format!(
            "Windows could not revalidate HWND {} ownership",
            target.id
        )));
    }
    if actual_pid != target.pid {
        return Err(no_window_error(format!(
            "The exact target HWND {} changed owner from PID {} to PID {}",
            target.id, target.pid, actual_pid
        )));
    }
    if unsafe { IsIconic(hwnd) }.as_bool() {
        return Err(no_window_error(format!(
            "The exact target HWND {} (PID {}) became minimized",
            target.id, target.pid
        )));
    }
    Ok(())
}

fn exact_window_geometry(target: &TargetBinding) -> Result<SourceGeometry, ComputerError> {
    exact_window_pair(target)?;
    let mut rect = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            target.hwnd(),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .map_err(|error| {
        no_window_error(format!(
            "Failed to re-read bounds for HWND {} (PID {}): {error}",
            target.id, target.pid
        ))
    })?;
    let width = positive_rect_dimension(rect.right, rect.left, "width")?;
    let height = positive_rect_dimension(rect.bottom, rect.top, "height")?;
    validate_source_dimensions(width, height).map_err(capture_error)?;
    Ok(SourceGeometry {
        x: rect.left,
        y: rect.top,
        width,
        height,
    })
}

fn positive_rect_dimension(high: i32, low: i32, name: &str) -> Result<u32, ComputerError> {
    let dimension = i64::from(high) - i64::from(low);
    u32::try_from(dimension)
        .ok()
        .filter(|dimension| *dimension != 0)
        .ok_or_else(|| capture_error(format!("The exact target window has an invalid {name}")))
}

fn validate_winrt_size(size: SizeInt32, stage: &str) -> Result<(u32, u32), String> {
    let width = u32::try_from(size.Width)
        .ok()
        .filter(|width| *width != 0)
        .ok_or_else(|| format!("{stage} returned an invalid width {}", size.Width))?;
    let height = u32::try_from(size.Height)
        .ok()
        .filter(|height| *height != 0)
        .ok_or_else(|| format!("{stage} returned an invalid height {}", size.Height))?;
    validate_source_dimensions(width, height)?;
    Ok((width, height))
}

fn validate_texture_desc(
    desc: &D3D11_TEXTURE2D_DESC,
    content_width: u32,
    content_height: u32,
) -> Result<(), String> {
    validate_source_dimensions(desc.Width, desc.Height)?;
    if desc.Width != content_width || desc.Height != content_height {
        return Err(format!(
            "WGC texture {}x{} did not match bounded ContentSize {}x{}",
            desc.Width, desc.Height, content_width, content_height
        ));
    }
    if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
        return Err(format!(
            "WGC texture format {} did not match requested BGRA8 format {}",
            desc.Format.0, DXGI_FORMAT_B8G8R8A8_UNORM.0
        ));
    }
    if desc.SampleDesc.Count != 1 || desc.ArraySize != 1 || desc.MipLevels != 1 {
        return Err("WGC texture layout was not a single-sample 2D RGBA surface".to_string());
    }
    Ok(())
}

fn startup_failure(
    operation: &str,
    error: windows::core::Error,
    cleanup_error: Option<windows::core::Error>,
) -> ComputerError {
    if let Some(cleanup_error) = cleanup_error {
        fatal_stop_error(format!(
            "WGC {operation} failed ({error}) and frame-pool rollback was unconfirmed ({cleanup_error})"
        ))
    } else {
        windows_capture_error_with_operation(operation, error)
    }
}

fn startup_runtime_failure(
    runtime: &mut CaptureRuntime,
    operation: &str,
    error: windows::core::Error,
) -> ComputerError {
    match runtime.shutdown() {
        Ok(()) => windows_capture_error_with_operation(operation, error),
        Err(cleanup_error) => fatal_stop_error(format!(
            "WGC {operation} failed ({error}) and startup rollback was unconfirmed ({cleanup_error})"
        )),
    }
}

fn windows_capture_error(error: windows::core::Error) -> ComputerError {
    capture_error(format!("Windows Graphics Capture failed: {error}"))
}

fn windows_capture_error_with_operation(
    operation: &str,
    error: windows::core::Error,
) -> ComputerError {
    capture_error(format!(
        "Windows Graphics Capture {operation} failed: {error}"
    ))
}

fn share_interval(fps: u64) -> Result<Duration, ComputerError> {
    if !(MIN_SHARE_FPS..=MAX_SHARE_FPS).contains(&fps) {
        return Err(capture_error(format!(
            "Native Windows sharing requires an FPS from {MIN_SHARE_FPS} through {MAX_SHARE_FPS}"
        )));
    }
    Ok(Duration::from_nanos(1_000_000_000 / fps))
}

fn copy_bgra_rows_to_rgba(
    raw: &[u8],
    width: u32,
    height: u32,
    row_pitch: u32,
) -> Result<RgbaImage, String> {
    copy_bgra_rows_to_rgba_bounded(raw, width, height, row_pitch, MAX_WGC_OUTPUT_PIXELS)
}

fn copy_bgra_rows_to_rgba_bounded(
    raw: &[u8],
    width: u32,
    height: u32,
    row_pitch: u32,
    max_output_pixels: u64,
) -> Result<RgbaImage, String> {
    let source_pixels = validate_source_dimensions(width, height)?;
    if max_output_pixels == 0 {
        return Err("WGC output pixel budget must be positive".to_string());
    }

    let packed_row_u32 = width
        .checked_mul(BYTES_PER_PIXEL)
        .ok_or_else(|| "WGC frame row size overflowed".to_string())?;
    if row_pitch < packed_row_u32 {
        return Err(format!(
            "WGC frame row pitch {row_pitch} is smaller than packed BGRA width {packed_row_u32}"
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

    if source_pixels <= max_output_pixels {
        let packed_len = packed_row
            .checked_mul(rows)
            .ok_or_else(|| "WGC packed frame size overflowed".to_string())?;
        let mut packed = vec![0_u8; packed_len];
        for row in 0..rows {
            let source_start = row * stride;
            let destination_start = row * packed_row;
            for column in 0..width as usize {
                let source = source_start + column * 4;
                let destination = destination_start + column * 4;
                packed[destination] = raw[source + 2];
                packed[destination + 1] = raw[source + 1];
                packed[destination + 2] = raw[source];
                packed[destination + 3] = raw[source + 3];
            }
        }
        return RgbaImage::from_raw(width, height, packed)
            .ok_or_else(|| "WGC packed RGBA dimensions were inconsistent".to_string());
    }

    let scale = (max_output_pixels as f64 / source_pixels as f64).sqrt();
    let output_width = (f64::from(width) * scale).floor().max(1.0) as u32;
    let output_height = (f64::from(height) * scale).floor().max(1.0) as u32;
    let output_len = usize::try_from(
        u64::from(output_width)
            .checked_mul(u64::from(output_height))
            .and_then(|pixels| pixels.checked_mul(u64::from(BYTES_PER_PIXEL)))
            .ok_or_else(|| "WGC bounded output size overflowed".to_string())?,
    )
    .map_err(|_| "WGC bounded output does not fit memory indexing".to_string())?;
    let mut output = vec![0_u8; output_len];
    bilinear_resize_strided_bgra_to_rgba(
        raw,
        width,
        height,
        stride,
        &mut output,
        output_width,
        output_height,
    );
    RgbaImage::from_raw(output_width, output_height, output)
        .ok_or_else(|| "WGC packed RGBA dimensions were inconsistent".to_string())
}

fn validate_source_dimensions(width: u32, height: u32) -> Result<u64, String> {
    if width == 0 || height == 0 {
        return Err("WGC returned an empty frame".to_string());
    }
    let source_pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "WGC source frame dimensions overflowed".to_string())?;
    if width > MAX_WGC_SOURCE_DIMENSION
        || height > MAX_WGC_SOURCE_DIMENSION
        || source_pixels > MAX_WGC_SOURCE_PIXELS
    {
        return Err(format!(
            "WGC source frame {width}x{height} exceeds the bounded capture budget"
        ));
    }
    Ok(source_pixels)
}

#[allow(clippy::too_many_arguments)]
fn bilinear_resize_strided_bgra_to_rgba(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    source_stride: usize,
    output: &mut [u8],
    output_width: u32,
    output_height: u32,
) {
    let x_scale = f64::from(source_width) / f64::from(output_width);
    let y_scale = f64::from(source_height) / f64::from(output_height);
    for output_y in 0..output_height {
        let source_y =
            ((f64::from(output_y) + 0.5) * y_scale - 0.5).clamp(0.0, f64::from(source_height - 1));
        let y0 = source_y.floor() as usize;
        let y1 = (y0 + 1).min(source_height as usize - 1);
        let y_weight = source_y - y0 as f64;
        for output_x in 0..output_width {
            let source_x = ((f64::from(output_x) + 0.5) * x_scale - 0.5)
                .clamp(0.0, f64::from(source_width - 1));
            let x0 = source_x.floor() as usize;
            let x1 = (x0 + 1).min(source_width as usize - 1);
            let x_weight = source_x - x0 as f64;
            let output_offset = (output_y as usize * output_width as usize + output_x as usize) * 4;
            for (output_channel, source_channel) in [2, 1, 0, 3].into_iter().enumerate() {
                let top_left = f64::from(source[y0 * source_stride + x0 * 4 + source_channel]);
                let top_right = f64::from(source[y0 * source_stride + x1 * 4 + source_channel]);
                let bottom_left = f64::from(source[y1 * source_stride + x0 * 4 + source_channel]);
                let bottom_right = f64::from(source[y1 * source_stride + x1 * 4 + source_channel]);
                let top = top_left + (top_right - top_left) * x_weight;
                let bottom = bottom_left + (bottom_right - bottom_left) * x_weight;
                output[output_offset + output_channel] =
                    (top + (bottom - top) * y_weight).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn compositor_captured_at(frame: &Direct3D11CaptureFrame) -> Result<Instant, String> {
    let timestamp = frame
        .SystemRelativeTime()
        .map_err(|error| format!("Failed to read the WGC compositor timestamp: {error}"))?;
    let mut counter = 0_i64;
    let mut frequency = 0_i64;
    if unsafe { QueryPerformanceCounter(&mut counter) } == 0
        || unsafe { QueryPerformanceFrequency(&mut frequency) } == 0
    {
        return Err("Failed to read the Windows performance counter".to_string());
    }
    let age = qpc_frame_age(timestamp.Duration, counter, frequency)?;
    Instant::now()
        .checked_sub(age)
        .ok_or_else(|| "The WGC compositor timestamp predates the monotonic clock".to_string())
}

fn qpc_frame_age(
    frame_timestamp_100ns: i64,
    current_counter: i64,
    counter_frequency: i64,
) -> Result<Duration, String> {
    if frame_timestamp_100ns < 0 || current_counter < 0 || counter_frequency <= 0 {
        return Err("Windows returned an invalid WGC/QPC timestamp".to_string());
    }
    // Keep both clocks in one rational QPC domain until after subtraction.
    // SystemRelativeTime is QPC time quantized to 100 ns. Converting the
    // current counter first truncates its remainder and can therefore make a
    // just-rendered frame appear in the future. A compositor timestamp can
    // also lead a later user-mode QPC sample on real Windows systems. Frame
    // age is an elapsed-time measurement, so preserve past age precisely and
    // saturate any future lead to zero at the callback receipt boundary.
    let frequency = i128::from(counter_frequency);
    let current_scaled = i128::from(current_counter)
        .checked_mul(HUNDRED_NANOSECONDS_PER_SECOND)
        .ok_or_else(|| "Windows QPC timestamp conversion overflowed".to_string())?;
    let frame_scaled = i128::from(frame_timestamp_100ns)
        .checked_mul(frequency)
        .ok_or_else(|| "Windows WGC timestamp conversion overflowed".to_string())?;
    let age_scaled = current_scaled
        .checked_sub(frame_scaled)
        .ok_or_else(|| "Windows WGC/QPC timestamp difference overflowed".to_string())?;
    if age_scaled <= 0 {
        return Ok(Duration::ZERO);
    }

    // Round upward so the conversion never understates frame age.
    let age_nanos = age_scaled
        .checked_mul(100)
        .and_then(|value| value.checked_add(frequency - 1))
        .and_then(|value| value.checked_div(frequency))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| "WGC compositor frame age exceeded the monotonic range".to_string())?;
    Ok(Duration::from_nanos(age_nanos))
}

fn no_window_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_NO_WINDOW", message)
}

fn capture_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_CAPTURE_FAILED", message)
}

fn fatal_stop_error(message: impl Into<String>) -> ComputerError {
    let error = ComputerError::new(FATAL_CAPTURE_STOP_CODE, message);
    let mut fatal = FATAL_CAPTURE_STOP
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if fatal.is_none() {
        *fatal = Some(error.clone());
    }
    error
}

pub(crate) fn take_fatal_stop_error() -> Option<ComputerError> {
    FATAL_CAPTURE_STOP
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_budget_reserves_bounded_rollback_and_watchdog_margin() {
        assert_eq!(STARTUP_TOTAL_TIMEOUT, Duration::from_secs(10));
        assert_eq!(STARTUP_ROLLBACK_RESERVE, OWNER_STOP_TIMEOUT);
        assert_eq!(
            STARTUP_TOTAL_TIMEOUT,
            OWNER_START_TIMEOUT + FIRST_FRAME_TIMEOUT + OWNER_STOP_TIMEOUT
        );
        assert!(CALLBACK_DRAIN_TIMEOUT < OWNER_STOP_TIMEOUT);
        assert!(STARTUP_TOTAL_TIMEOUT < Duration::from_secs(12));
    }

    #[test]
    fn startup_wait_is_clipped_before_the_rollback_reserve() {
        let deadline = Instant::now() + Duration::from_millis(100);
        let clipped = bounded_wait_timeout(deadline, Duration::from_secs(5))
            .expect("a future readiness deadline should have a wait budget");
        assert!(clipped <= Duration::from_millis(100));
        assert!(clipped > Duration::ZERO);
        assert!(bounded_wait_timeout(Instant::now(), Duration::from_secs(5)).is_none());
    }

    #[test]
    fn startup_owner_wait_refuses_to_join_past_its_deadline() {
        let (release_tx, release_rx) = mpsc::channel();
        let owner = thread::spawn(move || {
            let _ = release_rx.recv();
            Ok(())
        });

        assert!(!wait_for_owner_exit(
            &owner,
            Instant::now() + Duration::from_millis(20)
        ));
        release_tx
            .send(())
            .expect("the test owner should still be waiting");
        owner.join().expect("the test owner should exit").unwrap();
    }

    #[test]
    fn reported_owner_startup_failure_is_confirmed_without_fatal_reclassification() {
        let reported_error = capture_error("expected owner startup failure");
        let owner_error = reported_error.clone();
        let (command_sender, _command_receiver) = mpsc::channel();
        let mut capture = NativeShareCapture {
            command_sender: Some(command_sender),
            owner: Some(thread::spawn(move || Err(owner_error))),
            state: Arc::new(SharedCaptureState::new("42".to_string(), 7)),
            window_id: "42".to_string(),
            fps: 4,
        };

        capture
            .confirm_reported_startup_failure_before(
                Instant::now() + Duration::from_secs(1),
                &reported_error,
            )
            .expect("a matching reported startup failure should confirm bounded owner exit");
        assert!(capture.owner.is_none());
        assert!(capture.command_sender.is_none());
    }

    #[test]
    fn startup_descriptor_preserves_geometry_for_owner_side_revalidation() {
        let target = WindowDescriptor {
            id: "42".to_string(),
            pid: 7,
            app_name: "Fixture".to_string(),
            title: "Target".to_string(),
            x: -120,
            y: 40,
            width: 800,
            height: 600,
            minimized: false,
            focused: false,
        };

        let binding = validate_descriptor_geometry(&target)
            .expect("descriptor validation should not require a live HWND");
        assert_eq!(binding.hwnd_value, 42);
        assert_eq!(binding.pid, 7);
        assert_eq!(
            binding.expected_geometry,
            SourceGeometry {
                x: -120,
                y: 40,
                width: 800,
                height: 600,
            }
        );
    }

    #[test]
    fn direct_copy_swizzles_bgra_to_rgba_and_removes_stride_padding() {
        let raw = [
            3, 2, 1, 4, 7, 6, 5, 8, 90, 91, 92, 93, 11, 10, 9, 12, 15, 14, 13, 16, 94, 95, 96, 97,
        ];

        let image = copy_bgra_rows_to_rgba(&raw, 2, 2, 12).expect("padded BGRA frame should copy");

        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(
            image.as_raw(),
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn bgra_copy_rejects_invalid_stride_and_truncation() {
        assert!(copy_bgra_rows_to_rgba(&[0; 8], 2, 1, 7).is_err());
        assert!(copy_bgra_rows_to_rgba(&[0; 15], 2, 2, 8).is_err());
        assert!(copy_bgra_rows_to_rgba(&[], 0, 1, 0).is_err());
    }

    #[test]
    fn bounded_downscale_bilinearly_swizzles_bgra_to_rgba() {
        let raw = [
            0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
        ];
        let image = copy_bgra_rows_to_rgba_bounded(&raw, 4, 1, 16, 2)
            .expect("bounded BGRA downscale should succeed");
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(image.as_raw(), &[128, 128, 0, 255, 128, 128, 255, 255]);
    }

    #[test]
    fn bounded_downscale_honors_bgra_stride_padding() {
        let raw = [
            0, 0, 255, 255, 90, 91, 92, 93, 255, 0, 0, 255, 94, 95, 96, 97,
        ];
        let image = copy_bgra_rows_to_rgba_bounded(&raw, 1, 2, 8, 1)
            .expect("padded BGRA downscale should succeed");
        assert_eq!((image.width(), image.height()), (1, 1));
        assert_eq!(image.as_raw(), &[128, 0, 128, 255]);
    }

    #[test]
    fn source_dimensions_fail_before_unbounded_allocation() {
        let error = copy_bgra_rows_to_rgba(&[], 8_192, 8_192, 32_768).unwrap_err();
        assert!(error.contains("bounded capture budget"));
    }

    #[test]
    fn source_dimension_validation_covers_budget_edges_and_extremes() {
        assert_eq!(
            validate_source_dimensions(4_096, 4_096).unwrap(),
            MAX_WGC_SOURCE_PIXELS
        );
        assert_eq!(
            validate_source_dimensions(MAX_WGC_SOURCE_DIMENSION, 2_048).unwrap(),
            MAX_WGC_SOURCE_PIXELS
        );
        assert!(validate_source_dimensions(0, 1).is_err());
        assert!(validate_source_dimensions(MAX_WGC_SOURCE_DIMENSION + 1, 1).is_err());
        assert!(validate_source_dimensions(8_192, 2_049).is_err());
        assert!(validate_source_dimensions(u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn mapped_slice_length_rejects_driver_reported_stride_extremes() {
        assert_eq!(bounded_mapped_byte_len(640, 480, 2_560).unwrap(), 1_228_800);
        assert!(bounded_mapped_byte_len(640, 480, 2_559).is_err());
        assert!(bounded_mapped_byte_len(1, 1, MAX_WGC_ROW_PITCH + 1).is_err());
        assert!(bounded_mapped_byte_len(2_048, 8_192, MAX_WGC_ROW_PITCH).is_err());
    }

    #[test]
    fn initial_item_size_is_bounded_before_frame_pool_creation() {
        assert_eq!(
            validate_winrt_size(
                SizeInt32 {
                    Width: 4_096,
                    Height: 4_096,
                },
                "initial GraphicsCaptureItem::Size",
            )
            .unwrap(),
            (4_096, 4_096)
        );
        let error = validate_winrt_size(
            SizeInt32 {
                Width: 8_192,
                Height: 2_049,
            },
            "initial GraphicsCaptureItem::Size",
        )
        .unwrap_err();
        assert!(error.contains("bounded capture budget"));
    }

    #[test]
    fn resize_content_size_is_bounded_before_recreate() {
        assert!(
            validate_winrt_size(
                SizeInt32 {
                    Width: 8_192,
                    Height: 2_048,
                },
                "frame ContentSize",
            )
            .is_ok()
        );
        assert!(
            validate_winrt_size(
                SizeInt32 {
                    Width: 8_193,
                    Height: 1,
                },
                "frame ContentSize",
            )
            .is_err()
        );
        assert!(
            validate_winrt_size(
                SizeInt32 {
                    Width: -1,
                    Height: 1,
                },
                "frame ContentSize",
            )
            .is_err()
        );
    }

    #[test]
    fn texture_descriptor_must_match_bounded_content_before_staging() {
        let valid = D3D11_TEXTURE2D_DESC {
            Width: 640,
            Height: 480,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        assert!(validate_texture_desc(&valid, 640, 480).is_ok());

        let mut oversized = valid;
        oversized.Width = MAX_WGC_SOURCE_DIMENSION + 1;
        assert!(validate_texture_desc(&oversized, oversized.Width, 480).is_err());

        let mut mismatched = valid;
        mismatched.Height += 1;
        assert!(validate_texture_desc(&mismatched, 640, 480).is_err());
    }

    #[test]
    fn metadata_names_create_free_threaded_only_for_the_real_backend() {
        let capture = NativeShareCapture {
            command_sender: None,
            owner: None,
            state: Arc::new(SharedCaptureState::new("42".to_string(), 7)),
            window_id: "42".to_string(),
            fps: 4,
        };
        let metadata = capture.metadata();
        assert_eq!(
            metadata.backend,
            "windows-graphics-capture/create-free-threaded"
        );
        assert_eq!(metadata.selection_mode, "programmatic-exact-window");
    }

    #[test]
    fn qpc_frame_age_preserves_compositor_delay() {
        let age = qpc_frame_age(19_500_000, 2_000_000, 1_000_000)
            .expect("valid QPC timestamps should map");
        assert_eq!(age, Duration::from_millis(50));
        assert_eq!(
            qpc_frame_age(33, 10, 3_000_000).unwrap(),
            Duration::from_nanos(34),
            "age conversion must round upward without truncating the clocks first"
        );
        assert_eq!(
            qpc_frame_age(34, 10, 3_000_000).unwrap(),
            Duration::ZERO,
            "future compositor timestamps must saturate to receipt time"
        );
        assert_eq!(
            qpc_frame_age(i64::MAX, 0, 1).unwrap(),
            Duration::ZERO,
            "even a large future lead is zero elapsed age rather than a clock-domain failure"
        );
        assert!(qpc_frame_age(-1, 1, 1).is_err());
        assert!(qpc_frame_age(1, 1, 0).is_err());
        assert!(qpc_frame_age(0, i64::MAX, 1).is_err());
    }

    #[test]
    fn capacity_one_state_replaces_old_frames_and_counts_drop() {
        let state = SharedCaptureState::new("42".to_string(), 7);
        let first_capture = Instant::now();
        let geometry = SourceGeometry {
            x: 10,
            y: 20,
            width: 2,
            height: 1,
        };
        state
            .publish(RgbaImage::new(1, 1), first_capture, geometry)
            .expect("first publish should succeed");
        state
            .publish(
                RgbaImage::new(2, 1),
                first_capture + Duration::from_millis(100),
                geometry,
            )
            .expect("second publish should succeed");

        let frame = state
            .take_after(0)
            .expect("state should be readable")
            .expect("latest frame should exist");
        assert_eq!(frame.source_sequence, 2);
        assert_eq!(frame.image.width(), 2);
        assert_eq!((frame.source_x, frame.source_y), (10, 20));
        assert_eq!((frame.source_width, frame.source_height), (2, 1));
        assert_eq!(state.dropped_frames(), 1);
        assert!(
            state
                .take_after(2)
                .expect("state should be readable")
                .is_none()
        );
    }

    #[test]
    fn one_shot_frame_geometry_rebinds_the_observation_descriptor() {
        let frame = NativeShareFrame {
            image: RgbaImage::new(2, 1),
            source_sequence: 1,
            captured_at: Instant::now(),
            source_x: -120,
            source_y: 40,
            source_width: 800,
            source_height: 600,
        };
        let mut target = WindowDescriptor {
            id: "42".to_string(),
            pid: 7,
            app_name: "Fixture".to_string(),
            title: "Before resize".to_string(),
            x: 0,
            y: 0,
            width: 640,
            height: 480,
            minimized: false,
            focused: false,
        };

        bind_frame_geometry(&frame, &mut target);

        assert_eq!((target.x, target.y), (-120, 40));
        assert_eq!((target.width, target.height), (800, 600));
    }

    #[test]
    fn close_discards_buffered_frame_and_fails_closed() {
        let state = SharedCaptureState::new("42".to_string(), 7);
        state
            .publish(
                RgbaImage::new(1, 1),
                Instant::now(),
                SourceGeometry {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
            )
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

    #[test]
    fn unconfirmed_capture_stop_has_a_process_fatal_error_code() {
        let _ = take_fatal_stop_error();
        let error = fatal_stop_error("capture thread did not confirm shutdown");
        assert_eq!(error.code, FATAL_CAPTURE_STOP_CODE);
        assert!(error.message.contains("did not confirm shutdown"));
        let latched = take_fatal_stop_error().expect("fatal stop must be latched for the worker");
        assert_eq!(latched.code, FATAL_CAPTURE_STOP_CODE);
        assert!(take_fatal_stop_error().is_none());
    }
}
