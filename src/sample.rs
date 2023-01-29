use hound::SampleFormat;
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

#[derive(Default)]
pub struct SampleManager {
    samples: RwLock<HashMap<PathBuf, Arc<Sample>>>,
}

impl SampleManager {
    pub fn add(&self, path: &Path) -> Result<Arc<Sample>, ()> {
        if self.samples.read().unwrap().contains_key(path) {
            return Err(());
        }
        let extension = path
            .extension()
            .map(|ostr| ostr.to_ascii_lowercase().to_string_lossy().to_string());
        let sample = match extension.as_ref().map(|s| s.as_str()) {
            Some("wav") => Arc::new(Sample::from_wav_path(&path)),
            Some("flac") => Arc::new(Sample::from_flac_path(&path)),
            Some(unhandled) => unimplemented!("cannot read {}", unhandled),
            None => panic!(
                "Could not determine filetype for extension {:?}",
                path.extension()
            ),
        };
        self.samples
            .write()
            .unwrap()
            .insert(path.to_path_buf(), sample.clone());
        Ok(sample)
    }
}

pub struct Sample {
    pub path: PathBuf,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Default for Sample {
    fn default() -> Sample {
        Sample {
            path: PathBuf::default(),
            left: Vec::new(),
            right: Vec::new(),
        }
    }
}

impl std::fmt::Debug for Sample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let length = self.left.len().min(self.right.len());
        f.debug_struct("Sample")
            .field("path", &self.path)
            .field("length", &length)
            .finish()
    }
}

impl Sample {
    // TODO: Convert to the proper sample rate or store the sample rate.
    pub fn from_wav_path(p: &Path) -> Sample {
        let reader = hound::WavReader::open(p).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_format, SampleFormat::Int);
        let sample_count = reader.len() as usize / 2;
        let multiplier = 1.0 / 2f64.powi(reader.spec().bits_per_sample as i32 - 1);
        let mut samples_iter = reader.into_samples::<i32>();
        let mut get_next = || {
            let value = samples_iter.next().unwrap().unwrap() as f64;
            (value * multiplier) as f32
        };
        let mut left = vec![0.0; sample_count];
        let mut right = vec![0.0; sample_count];
        for i in 0..sample_count {
            left[i] = get_next();
            right[i] = get_next();
        }
        Sample {
            path: p.to_path_buf(),
            left,
            right,
        }
    }

    // TODO: Convert to the proper sample rate or store the sample rate.
    pub fn from_flac_path(p: &Path) -> Sample {
        let mut stream =
            flac::StreamReader::<File>::from_file(p.as_os_str().to_str().unwrap()).unwrap();
        let info = stream.info();
        assert_eq!(info.channels, 2, "Only 2 channels are supported.");
        assert_eq!(info.bits_per_sample, 24);
        let sample_count = info.total_samples as usize / info.channels as usize;
        let multiplier = 1.0 / 2f64.powi(info.bits_per_sample as i32 - 1);
        let mut samples = stream.iter::<i32>();
        let mut left = vec![0.0; sample_count];
        let mut right = vec![0.0; sample_count];
        let mut get_next = || {
            let value = samples.next().unwrap() as f64;
            (value * multiplier) as f32
        };
        for i in 0..sample_count {
            left[i] = get_next();
            right[i] = get_next();
        }
        Sample {
            path: p.to_path_buf(),
            left,
            right,
        }
    }
}
