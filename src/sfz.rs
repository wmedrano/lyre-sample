use log::*;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::{collections::HashMap, path::Path, sync::Arc};

use crate::{
    sample::{Sample, SampleManager},
    voice::{KeyState, Voice},
};

const SAMPLE_RATE: f64 = 44100.0;
const SECONDS_PER_SAMPLE: f64 = 1.0 / SAMPLE_RATE;

pub struct Instrument {
    // TODO: Support groups.

    // The regions.
    regions: Vec<Region>,

    // Maps from note to the voices associated with the note.
    voices: HashMap<wmidi::Note, arrayvec::ArrayVec<Voice, 8>>,
}

#[derive(Debug, Default)]
struct Region {
    sample: Arc<Sample>,
    sample_pitch_frequency: f64,
    low_key: u8,
    high_key: u8,
    low_velocity: u8,
    high_velocity: u8,
    /// The amp release, in seconds.
    ampeg_release: f32,
    loop_mode: sofiza::loop_mode,
}

fn seconds_to_samples(seconds: f64, sample_rate: f64) -> f64 {
    seconds * sample_rate
}

impl Region {
    pub fn press(&self, k: wmidi::Note, _: wmidi::Velocity) -> Voice {
        // TODO: Figure out how wmidi::Velocity should factor into volume.
        let volume = 0.4;
        let release_samples = seconds_to_samples(self.ampeg_release as f64, SAMPLE_RATE);
        // TODO: Use an exponential envelope for more natural decay.
        let release_effect = -(volume as f64 / release_samples) as f32;
        Voice {
            sample: self.sample.clone(),
            time: 0.0,
            time_delta: SECONDS_PER_SAMPLE * k.to_freq_f64() / self.sample_pitch_frequency,
            loop_mode: self.loop_mode,
            state: KeyState::Play,
            volume,
            release_effect,
        }
    }

    pub fn note_is_relevant(&self, note: wmidi::Note) -> bool {
        let note = note as u8;
        if note < self.low_key || note > self.high_key {
            return false;
        }
        true
    }

    pub fn is_relevant(&self, note: wmidi::Note, velocity: wmidi::Velocity) -> bool {
        let velocity = u8::from(velocity);
        if !self.note_is_relevant(note) {
            return false;
        }
        if velocity < self.low_velocity || velocity > self.high_velocity {
            return false;
        }
        true
    }
}

impl Instrument {
    pub fn from_path(p: &Path) -> Instrument {
        let config = sofiza::Instrument::from_file(p).unwrap();
        let samples = SampleManager::default();
        error!("Default path {:?}", config.default_path);
        assert_eq!(config.global.len(), 0);
        let regions = config
            .regions
            .par_iter()
            .map(|region_config| {
                let mut region = Region::default();
                for opcode in region_config.opcodes.values() {
                    match &opcode {
                        sofiza::Opcode::loop_mode(m) => region.loop_mode = *m,
                        sofiza::Opcode::sample(path) => {
                            let full_path = config.default_path.join(path);
                            region.sample = samples.add(&full_path).unwrap();
                        }
                        sofiza::Opcode::lokey(k) => region.low_key = *k,
                        sofiza::Opcode::hikey(k) => region.high_key = *k,
                        sofiza::Opcode::lovel(v) => region.low_velocity = *v,
                        sofiza::Opcode::hivel(v) => region.high_velocity = *v,
                        sofiza::Opcode::pitch_keycenter(k) => {
                            region.sample_pitch_frequency =
                                wmidi::Note::from_u8_lossy(*k).to_freq_f64()
                        }
                        sofiza::Opcode::ampeg_release(s) => region.ampeg_release = *s,
                        c => error!("Code {:?} is not supported.", c),
                    }
                }
                region
            })
            .collect();
        // assert_eq!(config.groups.len(), 0, "{:?}", config.groups);
        Instrument {
            regions,
            voices: HashMap::new(),
        }
    }

    pub fn play<'a, M: Clone + Iterator<Item = (usize, &'a [u8])>>(
        &mut self,
        midi: M,
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        let mut midi = midi.peekable();
        let sample_count = out_l.len().min(out_r.len());
        for i in 0..sample_count {
            while midi.peek().map(|(f, _)| i >= *f).unwrap_or(false) {
                let (_, data) = midi.next().unwrap();
                self.handle_midi(wmidi::MidiMessage::try_from(data).unwrap());
            }
            (out_l[i], out_r[i]) = self.next_sample();
        }
        self.voices.retain(|_, voices| {
            voices.retain(|v| match v.state {
                KeyState::Play => true,
                KeyState::Release => true,
                KeyState::Done => false,
            });
            !voices.is_empty()
        });
    }

    fn handle_midi(&mut self, message: wmidi::MidiMessage) {
        match message {
            wmidi::MidiMessage::NoteOff(_, n, _) => {
                if let Some(voices) = self.voices.get_mut(&n) {
                    for voice in voices.iter_mut() {
                        match voice.loop_mode {
                            sofiza::loop_mode::no_loop => voice.state = KeyState::Release,
                            sofiza::loop_mode::one_shot => (),
                            sofiza::loop_mode::loop_continuous => todo!(),
                            sofiza::loop_mode::loop_sustain => todo!(),
                        }
                    }
                }
            }
            wmidi::MidiMessage::NoteOn(_, n, v) => {
                for region in self.regions.iter_mut() {
                    if region.is_relevant(n, v) {
                        let voices = self.voices.entry(n).or_default();
                        voices.push(region.press(n, v));
                    }
                }
            }
            _ => (),
        }
    }

    fn next_sample(&mut self) -> (f32, f32) {
        let (mut l, mut r) = (0.0, 0.0);
        for voices in self.voices.values_mut() {
            for voice in voices.iter_mut() {
                if !voice.state.is_active() {
                    continue;
                }
                let sample_index = (voice.time * SAMPLE_RATE) as usize;
                let ratio = (voice.time * SAMPLE_RATE).fract() as f32;
                if sample_index >= voice.sample.left.len() {
                    voice.state = KeyState::Done;
                    continue;
                }
                if voice.state == KeyState::Release {
                    voice.volume += voice.release_effect;
                    if voice.volume.is_sign_negative() {
                        voice.state = KeyState::Done;
                        continue;
                    }
                }
                l += voice.volume * interpolate(&voice.sample.left, sample_index, ratio);
                r += voice.volume * interpolate(&voice.sample.right, sample_index, ratio);
                voice.time += voice.time_delta;
            }
        }
        (l, r)
    }
}

fn interpolate(data: &[f32], idx: usize, ratio: f32) -> f32 {
    let a = data.get(idx).copied().unwrap_or_default();
    let b = data.get(idx + 1).copied().unwrap_or_default();
    a * (1.0 - ratio) + b * ratio
}
