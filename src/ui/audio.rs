use std::io::Cursor;
use std::sync::Arc;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

/// High-performance, in-memory sound cues using rodio
pub struct AudioManager {
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    click_wav: Arc<Vec<u8>>,
    tick_wav: Arc<Vec<u8>>,
    pop_wav: Arc<Vec<u8>>,
    enabled: bool,
}

impl AudioManager {
    pub fn new(enabled: bool) -> Self {
        let (stream, stream_handle) = match OutputStream::try_default() {
            Ok((s, h)) => (Some(s), Some(h)),
            Err(e) => {
                tracing::warn!("Audio output device not available: {}. Audio feedback disabled.", e);
                (None, None)
            }
        };

        // Synthesize in-memory PCM WAV buffers
        let click_wav = Arc::new(Self::synthesize_click());
        let tick_wav = Arc::new(Self::synthesize_tick());
        let pop_wav = Arc::new(Self::synthesize_pop());

        Self {
            _stream: stream,
            stream_handle,
            click_wav,
            tick_wav,
            pop_wav,
            enabled,
        }
    }

    /// Play soft click/tock on quick settings tile toggle
    pub fn play_click(&self) {
        if !self.enabled {
            return;
        }
        self.play_wav_buffer(self.click_wav.clone());
    }

    /// Play subtle notch tick on volume/brightness slider step or boundary
    pub fn play_tick(&self) {
        if !self.enabled {
            return;
        }
        self.play_wav_buffer(self.tick_wav.clone());
    }

    /// Play delicate pop on window spawn / dismiss
    pub fn play_pop(&self) {
        if !self.enabled {
            return;
        }
        self.play_wav_buffer(self.pop_wav.clone());
    }

    fn play_wav_buffer(&self, buffer: Arc<Vec<u8>>) {
        if let Some(handle) = &self.stream_handle {
            let cursor = Cursor::new((*buffer).clone());
            if let Ok(source) = Decoder::new(cursor) {
                if let Ok(sink) = Sink::try_new(handle) {
                    sink.set_volume(0.35); // Pleasant, non-intrusive volume
                    sink.append(source);
                    sink.detach();
                }
            }
        }
    }

    /// Synthesize 15ms soft tock (exponentially decayed 700Hz -> 300Hz sine wave)
    fn synthesize_click() -> Vec<u8> {
        let sample_rate = 44100u32;
        let duration_secs = 0.015f32;
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let progress = t / duration_secs;
            let freq = 700.0 - 400.0 * progress;
            let envelope = (-12.0 * progress).exp();
            let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * envelope * 0.7;
            let sample_i16 = (sample * i16::MAX as f32) as i16;
            samples.push(sample_i16);
        }

        Self::create_wav(sample_rate, &samples)
    }

    /// Synthesize 5ms crisp notch tick (high 1300Hz sine burst)
    fn synthesize_tick() -> Vec<u8> {
        let sample_rate = 44100u32;
        let duration_secs = 0.005f32;
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let progress = t / duration_secs;
            let envelope = (1.0 - progress).max(0.0);
            let sample = (2.0 * std::f32::consts::PI * 1300.0 * t).sin() * envelope * 0.4;
            let sample_i16 = (sample * i16::MAX as f32) as i16;
            samples.push(sample_i16);
        }

        Self::create_wav(sample_rate, &samples)
    }

    /// Synthesize 25ms delicate pop (450Hz warm sine pulse)
    fn synthesize_pop() -> Vec<u8> {
        let sample_rate = 44100u32;
        let duration_secs = 0.025f32;
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let progress = t / duration_secs;
            let envelope = (std::f32::consts::PI * progress).sin();
            let sample = (2.0 * std::f32::consts::PI * 450.0 * t).sin() * envelope * 0.5;
            let sample_i16 = (sample * i16::MAX as f32) as i16;
            samples.push(sample_i16);
        }

        Self::create_wav(sample_rate, &samples)
    }

    fn create_wav(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
        let mut buf = Vec::new();
        let num_channels = 1u16;
        let bits_per_sample = 16u16;
        let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample as u32 / 8);
        let block_align = num_channels * (bits_per_sample / 8);
        let data_size = samples.len() as u32 * 2;
        let chunk_size = 36 + data_size;

        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&chunk_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size
        buf.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat (PCM)
        buf.extend_from_slice(&num_channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());

        for &s in samples {
            buf.extend_from_slice(&s.to_le_bytes());
        }

        buf
    }
}
