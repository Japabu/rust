//! Audio output.

use toyos_abi::syscall;

/// Write PCM audio samples (s16le stereo 44100Hz) to the sound device.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn write_samples(samples: &[u8]) {
    syscall::audio_write(samples);
}
