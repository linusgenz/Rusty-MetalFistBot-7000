use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use augmented_dsp_filters::rbj::{FilterProcessor, FilterType};
use audio_processor_traits::{AudioBuffer, AudioProcessor, AudioProcessorSettings, AudioContext};
use audio_processor_traits::simple_processor::MultiChannel;

#[derive(Clone, Debug)]
pub struct AudioFilterState {
    pub bass_boost: bool,
    pub nightcore: bool,
    pub vaporwave: bool,
    pub volume: f32,
}

impl Default for AudioFilterState {
    fn default() -> Self {
        Self {
            bass_boost: false,
            nightcore: false,
            vaporwave: false,
            volume: 1.0,
        }
    }
}

pub enum AudioCommand {
    ToggleBassBoost(bool),
    ToggleNightcore(bool),
    ToggleVaporwave(bool),
    SetVolume(f32),
}

pub type SharedAudioFilterState = Arc<RwLock<AudioFilterState>>;

pub struct AudioFilters {
    low_shelf: MultiChannel<FilterProcessor<f32>>,
    mid_band: MultiChannel<FilterProcessor<f32>>,
    context: AudioContext,
    compressor: Compressor,
}

impl AudioFilters {
    pub fn new(sample_rate: f32) -> Self {
        let build_low = move || {
            let mut f = FilterProcessor::<f32>::new(FilterType::LowShelf);
            f.set_sample_rate(sample_rate);
            f.set_cutoff(100.0);
            f.set_q(0.707);
            f.set_gain_db(9.0);
            f.setup();
            f
        };

        let build_mid = move || {
            let mut f = FilterProcessor::<f32>::new(FilterType::BandShelf);
            f.set_sample_rate(sample_rate);
            f.set_center_frequency(300.0);
            f.set_band_width(1.0); // Oktave breit
            f.set_gain_db(-6.0);   // senken
            f.setup();
            f
        };

        let mut low_shelf = MultiChannel::new(build_low);
        let mut mid_band = MultiChannel::new(build_mid);

        let settings = AudioProcessorSettings {
            sample_rate,
            ..AudioProcessorSettings::default()
        };
        let mut context = AudioContext::from(settings.clone());
        low_shelf.prepare(&mut context);
        mid_band.prepare(&mut context);

        Self {
            low_shelf,
            mid_band,
            context,
            compressor: Compressor::new(sample_rate, -10.0, 3.0, 0.005, 0.05),
        }
    }

    pub fn apply(&mut self, frame: &mut [i16], channels: usize) {
        if frame.is_empty() {
            return;
        }

        let num_samples = frame.len() / channels;
        let mut buffer = AudioBuffer::<f32>::empty();
        buffer.resize(channels, num_samples);


        for ch in 0..channels {
            for n in 0..num_samples {
                let idx = n * channels + ch;
                buffer.set(ch, n, frame[idx] as f32 / 32768.0);
            }
        }


        self.low_shelf.process(&mut self.context, &mut buffer);
        self.mid_band.process(&mut self.context, &mut buffer);

        // Kompressor
        for ch in 0..channels {
            for n in 0..num_samples {
                let s = buffer.get(ch, n);
                let c = self.compressor.process_sample(*s);
                buffer.set(ch, n, c);
            }
        }

        for ch in 0..channels {
            for n in 0..num_samples {
                let idx = n * channels + ch;
                frame[idx] = (buffer.get(ch, n) * 32768.0)
                    .clamp(i16::MIN as f32, i16::MAX as f32)
                    as i16;
            }
        }
    }
}

#[derive(Clone)]
pub struct Compressor {
    threshold: f32,  // dBFS
    ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,
    envelope: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32, threshold_db: f32, ratio: f32, attack: f32, release: f32) -> Self {
        Self {
            threshold: threshold_db,
            ratio,
            attack_coeff: (-1.0 / (attack * sample_rate)).exp(),
            release_coeff: (-1.0 / (release * sample_rate)).exp(),
            envelope: 0.0,
        }
    }

    pub fn process_sample(&mut self, x: f32) -> f32 {
        let input_db = 20.0 * x.abs().max(1e-6).log10();
        let over_db = input_db - self.threshold;
        let gain_reduction_db = if over_db > 0.0 {
            over_db - (over_db / self.ratio)
        } else {
            0.0
        };

        let target_env = gain_reduction_db / 20.0;
        self.envelope = if target_env < self.envelope {
            self.attack_coeff * (self.envelope - target_env) + target_env
        } else {
            self.release_coeff * (self.envelope - target_env) + target_env
        };

        x * 10f32.powf(-self.envelope)
    }
}

pub type SharedAudioFilters = Arc<Mutex<AudioFilters>>;
