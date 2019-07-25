use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

// TODO multichannel resamplers/mixers?
// TODO maybe dont mutex

struct Drain<T> {
    deque: Arc<Mutex<VecDeque<T>>>,
}

impl<T> Drain<T> {
    fn new(deque: Arc<Mutex<VecDeque<T>>>) -> Self {
        Drain { deque }
    }
}

impl<T> Iterator for Drain<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.deque.lock().unwrap().pop_front()
    }
}

pub trait Resampler: sample::Signal<Frame = [f32; 1]> {
    fn new(from: u32, to: u32) -> Self;
    fn push_sample(&mut self, sample: f32);
}

pub struct SincResampler {
    queue: Arc<Mutex<VecDeque<[f32; 1]>>>,
    converter: Option<
        sample::interpolate::Converter<
            sample::signal::FromIterator<Drain<[f32; 1]>>,
            sample::interpolate::Sinc<[[f32; 1]; 128]>,
        >,
    >,
}

impl Resampler for SincResampler {
    fn new(from: u32, to: u32) -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let interpolator =
            sample::interpolate::Sinc::new(sample::ring_buffer::Fixed::from([[0f32; 1]; 128]));
        let converter = if from != to {
            Some(sample::interpolate::Converter::from_hz_to_hz(
                sample::signal::from_iter(Drain::new(queue.clone())),
                interpolator,
                f64::from(from),
                f64::from(to),
            ))
        } else {
            None
        };
        SincResampler { queue, converter }
    }

    fn push_sample(&mut self, v: f32) {
        self.queue.lock().unwrap().push_back([v]);
    }
}

impl sample::Signal for SincResampler {
    type Frame = [f32; 1];
    fn next(&mut self) -> [f32; 1] {
        if let Some(conv) = &mut self.converter {
            conv.next()
        } else {
            // TODO dont panic
            self.queue.lock().unwrap().pop_front().unwrap()
        }
    }
}

pub struct LinearResampler {
    queue: Arc<Mutex<VecDeque<[f32; 1]>>>,
    converter: Option<
        sample::interpolate::Converter<
            sample::signal::FromIterator<Drain<[f32; 1]>>,
            sample::interpolate::Linear<[f32; 1]>,
        >,
    >,
}

impl Resampler for LinearResampler {
    fn new(from: u32, to: u32) -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let interpolator = sample::interpolate::Linear::new([0.0], [0.0]);
        let converter = if from != to {
            Some(sample::interpolate::Converter::from_hz_to_hz(
                sample::signal::from_iter(Drain::new(queue.clone())),
                interpolator,
                f64::from(from),
                f64::from(to),
            ))
        } else {
            None
        };
        LinearResampler { queue, converter }
    }

    fn push_sample(&mut self, v: f32) {
        self.queue.lock().unwrap().push_back([v]);
    }
}

impl sample::Signal for LinearResampler {
    type Frame = [f32; 1];
    fn next(&mut self) -> [f32; 1] {
        if let Some(conv) = &mut self.converter {
            conv.next()
        } else {
            // TODO dont panic
            self.queue.lock().unwrap().pop_front().unwrap()
        }
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

pub struct Mixer<T: Resampler> {
    sample_rate: u32,
    target_sample_rate: Option<u32>,
    submission_queue: crossbeam_channel::Receiver<Submission>,
    resamplers: HashMap<u32, T>,
}

impl<T: Resampler> Mixer<T> {
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

impl<T: Resampler> sample::Signal for Mixer<T> {
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
