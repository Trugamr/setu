//! WASAPI adapter for [`super::Output`].
//!
//! Shared mode asks `IAudioClient3` for the device's minimum engine period (often well under the
//! classic 10 ms) and keeps one period plus `queue_ms` queued. Exclusive mode bypasses the mixer
//! with one buffer of the device's minimum period. The render thread registers with MMCSS as
//! "Pro Audio".

use std::time::{Duration, Instant};

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, RPC_E_CHANGED_MODE, WAIT_EVENT, WAIT_OBJECT_0,
};
use windows::Win32::Media::Audio::{
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_E_DEVICE_INVALIDATED,
    AUDCLNT_E_UNSUPPORTED_FORMAT, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, DEVICE_STATE_ACTIVE, IAudioClient, IAudioClient3,
    IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, eConsole, eRender,
};
use windows::Win32::Media::KernelStreaming::{
    KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
};
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToBSTR};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};
use windows::Win32::System::Threading::{
    AVRT_PRIORITY_CRITICAL, AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
    AvSetMmThreadPriority, CreateEventW, SetEvent, WaitForMultipleObjects,
};
use windows::core::{Interface, w};

use super::{Devices, Error, Event, OutputFormat, OutputInfo, OutputOptions, Render};

/// Wait between attempts to reopen a failed device.
const RETRY: Duration = Duration::from_millis(500);
/// How often a stream that follows the default device checks whether the default changed.
const DEFAULT_CHECK: Duration = Duration::from_secs(1);
/// Longest wait for a device event before checking on things anyway.
const WAKE_MS: u32 = 500;

/// Signals the render thread to stop: a Win32 event, so it wakes at once from its wait.
pub struct Stop(OwnedEvent);

impl Stop {
    pub fn new() -> Result<Self, Error> {
        OwnedEvent::new(true).map(Stop)
    }

    pub fn signal(&self) {
        // Cannot fail for a valid handle.
        let _ = unsafe { SetEvent(self.0.0) };
    }

    /// Waits up to `timeout`; true when stop was signaled.
    fn wait(&self, timeout: Duration) -> bool {
        let ms = timeout.as_millis().try_into().unwrap_or(u32::MAX);
        unsafe { WaitForMultipleObjects(&[self.0.0], false, ms) == WAIT_OBJECT_0 }
    }
}

pub fn devices() -> Result<Devices, Error> {
    let _com = Com::init()?;
    let enumerator = enumerator()?;
    let names = active_devices(&enumerator)?
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    let default = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .ok()
        .map(|device| device_name(&device));
    Ok(Devices { names, default })
}

pub fn run(
    options: &OutputOptions,
    render: &mut dyn Render,
    report: &mut dyn FnMut(Event),
    stop: &Stop,
) -> Result<(), Error> {
    let _com = Com::init()?;
    let _priority = ProAudio::register();
    let enumerator = enumerator()?;
    let mut opened = false;
    loop {
        match play(&enumerator, options, render, report, stop, &mut opened) {
            Ok(Ended::Stopped) => return Ok(()),
            // Reopen on the new default without waiting.
            Ok(Ended::DefaultChanged) => continue,
            Err(e) if !opened => return Err(e),
            Err(e) => report(Event::Lost(e)),
        }
        if stop.wait(RETRY) {
            return Ok(());
        }
    }
}

enum Ended {
    Stopped,
    DefaultChanged,
}

/// Opens the device once and renders until stopped, the default changes, or the device fails.
fn play(
    enumerator: &IMMDeviceEnumerator,
    options: &OutputOptions,
    render: &mut dyn Render,
    report: &mut dyn FnMut(Event),
    stop: &Stop,
    opened: &mut bool,
) -> Result<Ended, Error> {
    let (device, name) = find_device(enumerator, options.device.as_deref())?;
    let opened_id = device_id(&device);
    let stream = if options.exclusive {
        Stream::exclusive(&device)?
    } else {
        Stream::shared(&device, options.queue_ms)?
    };
    let format = stream.output_format();
    render.open(format);
    let mut scratch = vec![0.0f32; stream.buffer_frames as usize * usize::from(format.channels)];

    // Exclusive mode needs the buffer filled before it starts.
    if stream.exclusive {
        stream.write(render, &mut scratch, stream.buffer_frames)?;
    }
    unsafe { stream.client.Start() }.map_err(describe)?;
    *opened = true;
    report(Event::Opened(stream.info(name)));
    let default_moved = || {
        options.device.is_none()
            && unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
                .is_ok_and(|default| device_id(&default) != opened_id)
    };
    let result = stream.render_until(render, &mut scratch, stop, default_moved);
    let _ = unsafe { stream.client.Stop() };
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Float32,
    Int16,
    /// Also 24-bit samples in 32-bit containers.
    Int32,
}

impl Encoding {
    fn name(self) -> &'static str {
        match self {
            Encoding::Float32 => "float32",
            Encoding::Int16 => "int16",
            Encoding::Int32 => "int32",
        }
    }
}

/// A device format as WASAPI takes it, and how its samples are encoded.
#[derive(Clone, Copy)]
struct WaveFormat {
    wave: WAVEFORMATEXTENSIBLE,
    encoding: Encoding,
}

impl WaveFormat {
    /// Copies a format the engine handed out, such as the mix format.
    ///
    /// # Safety
    /// `format` points to a valid `WAVEFORMATEX`, followed by `cbSize` extra bytes.
    unsafe fn read(format: *const WAVEFORMATEX) -> Result<Self, Error> {
        let head = unsafe { format.read_unaligned() };
        let mut wave = WAVEFORMATEXTENSIBLE {
            Format: head,
            ..Default::default()
        };
        let mut tag = u32::from(head.wFormatTag);
        if tag == WAVE_FORMAT_EXTENSIBLE && head.cbSize >= 22 {
            wave = unsafe { format.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
            let sub = wave.SubFormat;
            tag = if sub == KSDATAFORMAT_SUBTYPE_PCM {
                WAVE_FORMAT_PCM
            } else if sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                WAVE_FORMAT_IEEE_FLOAT
            } else {
                0
            };
        }
        let bits = head.wBitsPerSample;
        let encoding = match (tag, bits) {
            (WAVE_FORMAT_IEEE_FLOAT, 32) => Encoding::Float32,
            (WAVE_FORMAT_PCM, 16) => Encoding::Int16,
            (WAVE_FORMAT_PCM, 32) => Encoding::Int32,
            _ => {
                return Err(Error::new(format!(
                    "unsupported device format: tag {tag:#x}, {bits} bits"
                )));
            }
        };
        Ok(WaveFormat { wave, encoding })
    }

    /// A stereo format for exclusive mode.
    fn stereo(rate: u32, bits: u16, valid_bits: u16, encoding: Encoding) -> Self {
        let block_align = 2 * bits / 8;
        let wave = WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                nChannels: 2,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * u32::from(block_align),
                nBlockAlign: block_align,
                wBitsPerSample: bits,
                cbSize: 22,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: valid_bits,
            },
            dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            SubFormat: if encoding == Encoding::Float32 {
                KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
            } else {
                KSDATAFORMAT_SUBTYPE_PCM
            },
        };
        WaveFormat { wave, encoding }
    }

    fn as_ptr(&self) -> *const WAVEFORMATEX {
        (&raw const self.wave).cast()
    }

    fn rate(&self) -> u32 {
        self.wave.Format.nSamplesPerSec
    }

    fn channels(&self) -> u16 {
        self.wave.Format.nChannels
    }
}

/// An initialized, not yet started render stream.
struct Stream {
    client: IAudioClient,
    render_client: IAudioRenderClient,
    event: OwnedEvent,
    format: WaveFormat,
    exclusive: bool,
    buffer_frames: u32,
    period_frames: u32,
    /// Shared mode: frames kept queued in the device buffer.
    queue_frames: u32,
}

impl Stream {
    fn shared(device: &IMMDevice, queue_ms: f64) -> Result<Self, Error> {
        let client: IAudioClient =
            unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(describe)?;
        let mix = unsafe { client.GetMixFormat() }.map_err(describe)?;
        let format = unsafe { WaveFormat::read(mix) };
        unsafe { CoTaskMemFree(Some(mix.cast_const().cast())) };
        let format = format?;

        let (client, period_frames) = match Self::low_latency_shared(device, &format) {
            Some(opened) => opened,
            None => {
                let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
                unsafe {
                    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, format.as_ptr(), None)
                }
                .map_err(describe)?;
                let mut default_period = 0i64;
                unsafe { client.GetDevicePeriod(Some(&mut default_period), None) }
                    .map_err(describe)?;
                let frames = (default_period as f64 * f64::from(format.rate()) / 1e7).round();
                (client, frames as u32)
            }
        };
        let queue = period_frames + (queue_ms * f64::from(format.rate()) / 1000.0) as u32;
        Self::finish(client, format, false, period_frames, queue)
    }

    /// Initializes a shared stream at the engine's minimum period through `IAudioClient3`.
    fn low_latency_shared(device: &IMMDevice, format: &WaveFormat) -> Option<(IAudioClient, u32)> {
        let client: IAudioClient3 = unsafe { device.Activate(CLSCTX_ALL, None) }.ok()?;
        let (mut default, mut fundamental, mut min, mut max) = (0, 0, 0, 0);
        unsafe {
            client.GetSharedModeEnginePeriod(
                format.as_ptr(),
                &mut default,
                &mut fundamental,
                &mut min,
                &mut max,
            )
        }
        .ok()?;
        let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        unsafe { client.InitializeSharedAudioStream(flags, min, format.as_ptr(), None) }.ok()?;
        let mut current_format = std::ptr::null_mut();
        let mut current = 0;
        let period = match unsafe {
            client.GetCurrentSharedModeEnginePeriod(&mut current_format, &mut current)
        } {
            Ok(()) => {
                unsafe { CoTaskMemFree(Some(current_format.cast_const().cast())) };
                current
            }
            Err(_) => min,
        };
        Some((client.cast().ok()?, period))
    }

    fn exclusive(device: &IMMDevice) -> Result<Self, Error> {
        let candidates = [
            WaveFormat::stereo(48000, 32, 32, Encoding::Float32),
            WaveFormat::stereo(48000, 32, 24, Encoding::Int32),
            WaveFormat::stereo(48000, 32, 32, Encoding::Int32),
            WaveFormat::stereo(48000, 16, 16, Encoding::Int16),
            WaveFormat::stereo(44100, 32, 24, Encoding::Int32),
            WaveFormat::stereo(44100, 16, 16, Encoding::Int16),
        ];
        let mut last = Error::new("no exclusive-mode format to try");
        for format in candidates {
            match Self::try_exclusive(device, format) {
                Ok(stream) => return Ok(stream),
                Err(e) => last = e,
            }
        }
        Err(Error::new(format!("exclusive mode unavailable: {last}")))
    }

    fn try_exclusive(device: &IMMDevice, format: WaveFormat) -> Result<Self, Error> {
        let activate = || -> Result<IAudioClient, Error> {
            unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(describe)
        };
        let client = activate()?;
        unsafe { client.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, format.as_ptr(), None) }
            .ok()
            .map_err(describe)?;
        let mut min_period = 0i64;
        unsafe { client.GetDevicePeriod(None, Some(&mut min_period)) }.map_err(describe)?;
        let initialize = |client: &IAudioClient, period: i64| unsafe {
            let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            client.Initialize(
                AUDCLNT_SHAREMODE_EXCLUSIVE,
                flags,
                period,
                period,
                format.as_ptr(),
                None,
            )
        };
        let client = match initialize(&client, min_period) {
            Ok(()) => client,
            Err(e) if e.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                // Retry with the period rounded to the device's buffer alignment.
                let frames = unsafe { client.GetBufferSize() }.map_err(describe)?;
                let period = (1e7 * f64::from(frames) / f64::from(format.rate())).round() as i64;
                let client = activate()?;
                initialize(&client, period).map_err(describe)?;
                client
            }
            Err(e) => return Err(describe(e)),
        };
        let stream = Self::finish(client, format, true, 0, 0)?;
        Ok(Stream {
            period_frames: stream.buffer_frames,
            queue_frames: stream.buffer_frames,
            ..stream
        })
    }

    /// Wires up the event and render client of an initialized client.
    fn finish(
        client: IAudioClient,
        format: WaveFormat,
        exclusive: bool,
        period_frames: u32,
        queue_frames: u32,
    ) -> Result<Self, Error> {
        let event = OwnedEvent::new(false)?;
        unsafe { client.SetEventHandle(event.0) }.map_err(describe)?;
        let buffer_frames = unsafe { client.GetBufferSize() }.map_err(describe)?;
        let render_client = unsafe { client.GetService() }.map_err(describe)?;
        Ok(Stream {
            client,
            render_client,
            event,
            format,
            exclusive,
            buffer_frames,
            period_frames,
            queue_frames: queue_frames.min(buffer_frames),
        })
    }

    fn output_format(&self) -> OutputFormat {
        OutputFormat {
            sample_rate: self.format.rate(),
            channels: self.format.channels(),
            period_frames: self.period_frames,
        }
    }

    fn info(&self, device: String) -> OutputInfo {
        let ms = |frames: u32| f64::from(frames) * 1000.0 / f64::from(self.format.rate());
        let engine_latency = unsafe { self.client.GetStreamLatency() }.unwrap_or(0);
        OutputInfo {
            device,
            exclusive: self.exclusive,
            format: self.output_format(),
            encoding: self.format.encoding.name().into(),
            buffer_ms: ms(self.buffer_frames),
            queued_ms: ms(self.queue_frames),
            engine_latency_ms: engine_latency as f64 / 1e4,
        }
    }

    /// Renders on every device event until stopped or the device fails. Once a second it asks
    /// `moved` whether the stream should reopen on another device.
    fn render_until(
        &self,
        render: &mut dyn Render,
        scratch: &mut [f32],
        stop: &Stop,
        mut moved: impl FnMut() -> bool,
    ) -> Result<Ended, Error> {
        let mut next_check = Instant::now() + DEFAULT_CHECK;
        loop {
            let woke = unsafe { WaitForMultipleObjects(&[stop.0.0, self.event.0], false, WAKE_MS) };
            if woke == WAIT_OBJECT_0 {
                return Ok(Ended::Stopped);
            }
            if Instant::now() >= next_check {
                next_check = Instant::now() + DEFAULT_CHECK;
                if moved() {
                    return Ok(Ended::DefaultChanged);
                }
            }
            if woke == WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
                let frames = self.writable_frames()?;
                if frames > 0 {
                    self.write(render, scratch, frames)?;
                }
            }
        }
    }

    /// Frames to write after a device event.
    fn writable_frames(&self) -> Result<u32, Error> {
        if self.exclusive {
            return Ok(self.buffer_frames);
        }
        let padding = unsafe { self.client.GetCurrentPadding() }.map_err(describe)?;
        Ok(self.queue_frames.saturating_sub(padding))
    }

    fn write(
        &self,
        render: &mut dyn Render,
        scratch: &mut [f32],
        frames: u32,
    ) -> Result<(), Error> {
        let samples = frames as usize * usize::from(self.format.channels());
        let data = unsafe { self.render_client.GetBuffer(frames) }.map_err(describe)?;
        let scratch = &mut scratch[..samples];
        render.render(scratch);
        // The engine hands out buffers aligned for its sample type.
        unsafe {
            match self.format.encoding {
                Encoding::Float32 => {
                    std::slice::from_raw_parts_mut(data.cast::<f32>(), samples)
                        .copy_from_slice(scratch);
                }
                Encoding::Int16 => {
                    let out = std::slice::from_raw_parts_mut(data.cast::<i16>(), samples);
                    for (o, s) in out.iter_mut().zip(scratch.iter()) {
                        *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                }
                Encoding::Int32 => {
                    let out = std::slice::from_raw_parts_mut(data.cast::<i32>(), samples);
                    for (o, s) in out.iter_mut().zip(scratch.iter()) {
                        *o = (f64::from(s.clamp(-1.0, 1.0)) * 2_147_483_647.0) as i32;
                    }
                }
            }
            self.render_client.ReleaseBuffer(frames, 0)
        }
        .map_err(describe)
    }
}

fn enumerator() -> Result<IMMDeviceEnumerator, Error> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(describe)
}

fn active_devices(enumerator: &IMMDeviceEnumerator) -> Result<Vec<(IMMDevice, String)>, Error> {
    let collection =
        unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }.map_err(describe)?;
    let count = unsafe { collection.GetCount() }.map_err(describe)?;
    (0..count)
        .map(|i| {
            let device = unsafe { collection.Item(i) }.map_err(describe)?;
            let name = device_name(&device);
            Ok((device, name))
        })
        .collect()
}

/// The default device, or the first whose name contains `query`, ignoring case.
fn find_device(
    enumerator: &IMMDeviceEnumerator,
    query: Option<&str>,
) -> Result<(IMMDevice, String), Error> {
    let Some(query) = query else {
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
            .map_err(|e| Error::new(format!("no default output device: {}", describe(e))))?;
        let name = device_name(&device);
        return Ok((device, name));
    };
    let query = query.to_lowercase();
    active_devices(enumerator)?
        .into_iter()
        .find(|(_, name)| name.to_lowercase().contains(&query))
        .ok_or_else(|| Error::new(format!("no output device matching \"{query}\"")))
}

fn device_name(device: &IMMDevice) -> String {
    let name = || -> windows::core::Result<String> {
        unsafe {
            let store = device.OpenPropertyStore(STGM_READ)?;
            let mut value = store.GetValue(&PKEY_Device_FriendlyName)?;
            let name = PropVariantToBSTR(&value).map(|b| b.to_string());
            let _ = PropVariantClear(&mut value);
            name
        }
    };
    name().unwrap_or_else(|_| "unknown device".into())
}

fn device_id(device: &IMMDevice) -> Option<String> {
    unsafe {
        let id = device.GetId().ok()?;
        let text = id.to_string().ok();
        CoTaskMemFree(Some(id.0.cast_const().cast()));
        text
    }
}

/// Turns the WASAPI failures people are likely to hit into actionable messages.
fn describe(e: windows::core::Error) -> Error {
    let hint = match e.code() {
        AUDCLNT_E_DEVICE_IN_USE => "the device is in use exclusively by another app",
        AUDCLNT_E_UNSUPPORTED_FORMAT => "the device does not support the format",
        AUDCLNT_E_DEVICE_INVALIDATED => "the device was removed, disabled or taken over",
        _ => return Error::new(e.message()),
    };
    Error::new(format!("{hint} ({:#010x})", e.code().0))
}

struct OwnedEvent(HANDLE);

// A Win32 event handle may be waited on and signaled from any thread.
unsafe impl Send for OwnedEvent {}
unsafe impl Sync for OwnedEvent {}

impl OwnedEvent {
    fn new(manual_reset: bool) -> Result<Self, Error> {
        unsafe { CreateEventW(None, manual_reset, false, None) }
            .map(OwnedEvent)
            .map_err(|e| Error::new(format!("could not create an event: {e}")))
    }
}

impl Drop for OwnedEvent {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// COM for the current thread, released on drop.
struct Com {
    initialized: bool,
}

impl Com {
    fn init() -> Result<Self, Error> {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr == RPC_E_CHANGED_MODE {
            // The thread already runs COM in another mode, which works for WASAPI too.
            return Ok(Com { initialized: false });
        }
        hr.ok().map_err(describe)?;
        Ok(Com { initialized: true })
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

/// The current thread registered with MMCSS as "Pro Audio", reverted on drop. Without MMCSS
/// the thread keeps its normal priority.
struct ProAudio(Option<HANDLE>);

impl ProAudio {
    fn register() -> Self {
        let mut index = 0;
        let handle = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) }.ok();
        if let Some(handle) = handle {
            let _ = unsafe { AvSetMmThreadPriority(handle, AVRT_PRIORITY_CRITICAL) };
        }
        ProAudio(handle)
    }
}

impl Drop for ProAudio {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            let _ = unsafe { AvRevertMmThreadCharacteristics(handle) };
        }
    }
}
