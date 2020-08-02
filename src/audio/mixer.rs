use std::collections::HashMap;

pub struct SubmissionBuilder {
    channels: usize,
    rates: Vec<u32>,
}

impl SubmissionBuilder {
    /// length is in secs
    pub fn create(&self, length: f32) -> Submission {
        tracing::trace!(length = %length, "Creating new mixer submission");

        let mut streams = HashMap::new();

        for rate in &self.rates {
            if !streams.contains_key(rate) {
                let stream = vec![0f32; ((*rate as f32) * length) as usize * self.channels];
                streams.insert(*rate, stream);
            }
        }

        Submission {
            streams,
            channels: self.channels,
            length,
        }
    }
}

pub struct Submission {
    streams: HashMap<u32, Vec<f32>>,
    channels: usize,
    length: f32,
}

impl Submission {
    pub fn add<I: IntoIterator<Item = f32>>(&mut self, rate: u32, channel: usize, samples: I) {
        let sp = tracing::trace_span!("write_to_submission");
        let _e = sp.enter();

        if channel >= self.channels {
            tracing::warn!(
                "Writing to nonexistent channel {}, previous channels will be overwritten!",
                channel
            );
        }

        let mut sample_iter = samples.into_iter();
        match self.streams.get_mut(&rate) {
            Some(stream) => {
                for v in stream.iter_mut().skip(channel).step_by(self.channels) {
                    *v += sample_iter.next().unwrap_or(0.0);
                }
            }
            None => tracing::warn!("Submission has no {}hz stream!", rate),
        }
    }

    pub fn length_of_channel(&self, rate: u32) -> Option<usize> {
        self.streams
            .get(&rate)
            .map(Vec::len)
            .map(|v| v / self.channels)
    }
}

pub struct MixerBuilder {
    channels: usize,
    sample_rate: Option<u32>,
    conv_type: samplerate::ConverterType,
    source_rates: Vec<u32>,
}

impl MixerBuilder {
    pub fn new() -> Self {
        MixerBuilder {
            channels: 1,
            sample_rate: None,
            conv_type: samplerate::ConverterType::SincFastest,
            source_rates: Vec::new(),
        }
    }

    pub fn channels(&mut self, channels: usize) -> &mut Self {
        self.channels = channels;
        self
    }

    pub fn target_sample_rate(&mut self, rate: u32) -> &mut Self {
        self.sample_rate = Some(rate);
        self
    }

    pub fn resample_type(&mut self, ty: samplerate::ConverterType) -> &mut Self {
        self.conv_type = ty;
        self
    }

    pub fn source_rate(&mut self, rate: u32) -> &mut Self {
        self.source_rates.push(rate);
        self
    }

    pub fn build<I: Iterator<Item = Submission>>(
        self,
        source: I,
    ) -> Result<Mixer<I>, samplerate::Error> {
        let sp = tracing::trace_span!("build_mixer");
        let _e = sp.enter();

        let sample_rate = match self.sample_rate {
            Some(r) => r,
            None => *self.source_rates.iter().max().unwrap_or_else(|| {
                tracing::warn!("Mixer was given no source sample rates! Defaulting to 44100...");
                &44100
            }),
        };

        let mut converters = HashMap::new();
        for rate in self.source_rates {
            if rate != sample_rate {
                let converter =
                    samplerate::Samplerate::new(self.conv_type, rate, sample_rate, self.channels)?;
                converters.entry(rate).or_insert(Some(converter));
            } else {
                converters.entry(rate).or_insert(None);
            }
        }

        Ok(Mixer {
            submission_queue: source,
            channels: self.channels,
            sample_rate,
            converters,
        })
    }
}

pub type MixerStream<I> = std::iter::Flatten<Mixer<I>>;

pub struct Mixer<I: Iterator<Item = Submission>> {
    submission_queue: I,
    channels: usize,
    sample_rate: u32,
    converters: HashMap<u32, Option<samplerate::Samplerate>>,
}

impl<I: Iterator<Item = Submission>> Mixer<I> {
    pub fn submission_builder(&self) -> SubmissionBuilder {
        SubmissionBuilder {
            channels: self.channels,
            rates: self.converters.keys().copied().collect(),
        }
    }

    pub fn submission_queue(&mut self) -> &mut I {
        &mut self.submission_queue
    }

    pub fn into_stream(self) -> MixerStream<I> {
        self.flatten()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

impl<I: Iterator<Item = Submission>> Iterator for Mixer<I> {
    type Item = Vec<f32>;

    fn next(&mut self) -> Option<Vec<f32>> {
        let submission = self.submission_queue.next()?;
        let n_streams = submission.streams.len();

        // TODO report errors?
        let mut resampled_streams = submission.streams.into_iter().filter_map(|(rate, stream)| {
            let resampler = self.converters.get(&rate)?;

            match resampler {
                Some(r) => r.process(&stream).ok(),
                None => Some(stream),
            }
        });

        if n_streams == 1 {
            Some(resampled_streams.next().unwrap())
        } else {
            let chunk_len = (self.sample_rate as f32 * submission.length) as usize * self.channels;
            let mut chunk = vec![0f32; chunk_len];

            for stream in resampled_streams {
                let mut stream_iter = stream.iter();
                for v in chunk.iter_mut() {
                    *v += stream_iter.next().unwrap_or(&0f32);
                }
            }

            Some(chunk)
        }
    }
}

// TODO !!!!! VERIFY THIS !!!!!
unsafe impl<I: Iterator<Item = Submission>> Send for Mixer<I> {}
unsafe impl<I: Iterator<Item = Submission>> Sync for Mixer<I> {}
