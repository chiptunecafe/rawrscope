use std::collections::HashMap;

// TODO multichannel resamplers/mixers?

type CrossbeamSignal<T> = sample::signal::FromIterator<crossbeam_channel::IntoIter<T>>;

struct Resampler {
    input: crossbeam_channel::Sender<[f32; 1]>,
    converter: sample::interpolate::Converter<
        CrossbeamSignal<[f32; 1]>,
        sample::interpolate::Sinc<[[f32; 1]; 128]>,
    >,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        // TODO bounded channel?
        let (tx, rx) = crossbeam_channel::unbounded();
        tx.send([0.0; 1]).unwrap(); // workaround for converter weirdness
        let interpolator =
            sample::interpolate::Sinc::new(sample::ring_buffer::Fixed::from([[0f32; 1]; 128]));
        Resampler {
            input: tx,
            converter: sample::interpolate::Converter::from_hz_to_hz(
                sample::signal::from_iter(rx.into_iter()),
                interpolator,
                f64::from(from),
                f64::from(to),
            ),
        }
    }

    pub fn push_sample(&mut self, v: f32) {
        // TODO do not panic
        self.input.send([v]).expect("could not send sample");
    }
}

impl sample::Signal for Resampler {
    type Frame = [f32; 1];
    fn next(&mut self) -> [f32; 1] {
        self.converter.next()
    }
}

pub struct MixedStream {
    pub mixed: Vec<f32>,
    pub num_streams: usize,
}

pub struct Submission(HashMap<u32, MixedStream>);

impl Submission {
    pub fn new() -> Self {
        Submission(HashMap::new())
    }

    // TODO possibly wonky treatment of differently sized streams
    pub fn add(&mut self, sample_rate: u32, samples: Vec<f32>) {
        match self.0.get_mut(&sample_rate) {
            Some(stream) => {
                stream.num_streams += 1;
                for (i, v) in stream.mixed.iter_mut().enumerate() {
                    if i < samples.len() {
                        *v += samples[i];
                    }
                }
            }
            None => {
                let stream = MixedStream {
                    mixed: samples,
                    num_streams: 1,
                };
                self.0.insert(sample_rate, stream);
            }
        }
    }
}

// TODO dont resample streams that are already target sample rate
pub struct Mixer {
    sample_rate: u32,
    target_sample_rate: Option<u32>,
    submission_queue: crossbeam_channel::Receiver<Submission>,
    resamplers: HashMap<u32, Resampler>,
}

impl Mixer {
    pub fn new(target_sample_rate: Option<u32>) -> (Self, crossbeam_channel::Sender<Submission>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (
            Mixer {
                sample_rate: target_sample_rate.unwrap_or(44100),
                target_sample_rate,
                submission_queue: rx,
                resamplers: HashMap::new(),
            },
            tx,
        )
    }
}

impl sample::Signal for Mixer {
    type Frame = [f32; 1];
    fn next(&mut self) -> [f32; 1] {
        // poll for new submission
        if let Ok(sub) = self.submission_queue.try_recv() {
            // determine optimal sample rate if not forced
            if self.target_sample_rate.is_none() {
                let mut rate = self.sample_rate;
                let mut num_streams = 0;

                let rates = sub.0.iter().map(|(rate, mix)| (rate, mix.num_streams));

                for (new_rate, new_streams) in rates {
                    if new_streams > num_streams || (new_streams == num_streams && *new_rate > rate)
                    {
                        rate = *new_rate;
                        num_streams = new_streams;
                    }
                }

                log::debug!("New mixer sample rate: {}", rate);
                self.sample_rate = rate;
                // must recreate all resamplers
                self.resamplers.clear();
            }

            // create new resamplers TODO remove old ones
            for rate in sub.0.keys() {
                if !self.resamplers.contains_key(rate) {
                    log::debug!("Creating new resampler: {} => {}", rate, self.sample_rate);
                    self.resamplers
                        .insert(*rate, Resampler::new(*rate, self.sample_rate));
                }
            }

            // push submitted samples to resamplers
            for (rate, samples) in sub.0.iter() {
                // just ignore if we dont have a resampler for some reason
                match self.resamplers.get_mut(rate) {
                    Some(r) => {
                        for s in &samples.mixed {
                            r.push_sample(*s);
                        }
                    }
                    None => log::warn!("Missing resampler!"),
                }
            }
        }

        // read and mix streams
        let sample = self
            .resamplers
            .values_mut()
            .map(|r| r.next()[0])
            .sum::<f32>();

        [sample; 1]
    }
}
