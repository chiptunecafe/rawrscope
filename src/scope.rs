use serde::{Deserialize, Serialize};

use crate::audio::mixer;
use crate::state::GridRect;

pub mod centering;
use centering::Algorithm;

// custom impl of std::option::IntoIter in order to expose inner value
struct SubmissionSlot {
    inner: Option<mixer::Submission>,
}

impl SubmissionSlot {
    pub fn new(inner: Option<mixer::Submission>) -> Self {
        SubmissionSlot { inner }
    }

    pub fn submission(&mut self) -> &mut Option<mixer::Submission> {
        &mut self.inner
    }
}

impl Iterator for SubmissionSlot {
    type Item = mixer::Submission;

    #[inline]
    fn next(&mut self) -> Option<mixer::Submission> {
        self.inner.take()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.inner {
            Some(_) => (1, Some(1)),
            None => (0, Some(0)),
        }
    }
}

// TODO allow for multiple channels
#[derive(Serialize, Deserialize)]
pub struct Scope {
    pub window_size: f32,

    // appearance
    pub line_width: f32,
    pub rect: GridRect,

    pub trigger_width: f32,
    pub centering: centering::Centering,

    #[serde(skip)]
    mixer: Option<mixer::Mixer<SubmissionSlot>>,

    #[serde(skip)]
    audio: Vec<f32>,

    #[serde(skip)]
    center_offset: usize,
}

impl Scope {
    pub fn wanted_length(&self) -> f32 {
        self.window_size + self.trigger_width
    }

    pub fn configure_mixer(&mut self, source_rates: Vec<u32>) {
        let mut mixer_builder = mixer::MixerBuilder::new();
        mixer_builder.channels(1);
        mixer_builder.resample_type(samplerate::ConverterType::Linear);

        for &rate in &source_rates {
            mixer_builder.source_rate(rate);
        }

        // TODO dont panic?
        self.mixer = Some(mixer_builder.build(SubmissionSlot::new(None)).unwrap());
    }

    // TODO maybe dont panic on these three methods
    pub fn build_submission(&self) -> mixer::Submission {
        self.mixer
            .as_ref()
            .expect("scope mixer unconfigured!")
            .submission_builder()
            .create(self.wanted_length())
    }

    pub fn submit(&mut self, submission: mixer::Submission) {
        self.mixer
            .as_mut()
            .expect("scope mixer unconfigured!")
            .submission_queue()
            .submission()
            .replace(submission);
    }

    // centering happens here
    pub fn process(&mut self) {
        let mixer = self.mixer.as_mut().expect("scope mixer unconfigured");
        let sample_rate = mixer.sample_rate();
        let output_size = (sample_rate as f32 * self.window_size) as usize;

        self.audio = mixer.next().expect("attempted to process no audio!");

        let trigger_samples = (sample_rate as f32 * self.trigger_width) as usize;
        let trigger_pad = (self.audio.len() - trigger_samples) / 2;
        let trigger_range = trigger_pad..=self.audio.len() - trigger_pad;

        let center = self.centering.center(&self.audio, &trigger_range);
        assert!(trigger_range.contains(&center));

        self.center_offset = center - output_size / 2;
    }

    pub fn output(&self) -> &[f32] {
        let output_size = (self
            .mixer
            .as_ref()
            .expect("scope mixer unconfigured")
            .sample_rate() as f32
            * self.window_size) as usize;

        &self.audio[self.center_offset..output_size + self.center_offset]
    }
}
