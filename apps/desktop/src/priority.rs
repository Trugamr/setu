//! Thread priority for the audio path. On Windows a thread registers with MMCSS as "Pro Audio",
//! which keeps it scheduled ahead of ordinary work while the PC is busy, such as during a game.
//! Elsewhere registering does nothing.

/// Where a thread ranks among the "Pro Audio" threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Priority {
    /// Feeds the device: any delay is an audible gap.
    Critical,
    /// Receives the stream: a delay counts as network jitter, which the buffer covers with
    /// latency.
    High,
}

/// The current thread's registration, reverted on drop. Without MMCSS the thread keeps its
/// normal priority.
pub use platform::ProAudio;

#[cfg(windows)]
mod platform {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::{
        AVRT_PRIORITY_CRITICAL, AVRT_PRIORITY_HIGH, AvRevertMmThreadCharacteristics,
        AvSetMmThreadCharacteristicsW, AvSetMmThreadPriority,
    };
    use windows::core::w;

    use super::Priority;

    pub struct ProAudio(Option<HANDLE>);

    impl ProAudio {
        pub fn register(priority: Priority) -> Self {
            let mut index = 0;
            let handle = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) }.ok();
            if let Some(handle) = handle {
                let level = match priority {
                    Priority::Critical => AVRT_PRIORITY_CRITICAL,
                    Priority::High => AVRT_PRIORITY_HIGH,
                };
                let _ = unsafe { AvSetMmThreadPriority(handle, level) };
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
}

#[cfg(not(windows))]
mod platform {
    use super::Priority;

    pub struct ProAudio;

    impl ProAudio {
        pub fn register(_: Priority) -> Self {
            ProAudio
        }
    }
}
