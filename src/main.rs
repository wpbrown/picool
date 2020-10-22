use anyhow::Result;
use log::*;
use std::{num::FpCategory, collections::VecDeque, env, mem::replace, ops::Range, path::PathBuf, time::Duration, time::Instant};
use strum_macros::Display;

cfg_if::cfg_if! {
    if #[cfg(feature = "demo-mode")] {
        mod demo_world;
        use demo_world::DemoWorld;
    } else {
        mod real_world;
        use real_world::RealWorld;
    }
}

const TARGET_RANGE: Range<f32> = 1.3..4.4;
const MAX_COMPENSATION: f32 = 1.6;
const MINIMUM_ON_DURATION: Duration = Duration::from_secs(60 * 2);
const MINIMUM_OFF_DURATION: Duration = Duration::from_secs(60 * 8);
const POLL_DURATION: Duration = Duration::from_secs(10);

trait World {
    fn get_temperature(&self) -> Result<f32>;
    fn set_power_state(&mut self, state: bool);
    fn sleep(&self, duration: Duration);
    fn now(&self) -> Instant;

    fn restore_power_state(&self) -> Result<RestoredPowerState>;
    fn persist_last_off_transition(&mut self) -> Result<()>;
}

#[derive(Eq, PartialEq, Copy, Clone, Display)]
enum State {
    InitiallyOff,
    MinimumIntervalOn(Instant),
    MinimumIntervalOff(Instant),
    On,
    Off,
}

#[derive(Eq, PartialEq, Copy, Clone, Display)]
enum RestoredPowerState {
    CurrentlyOn,
    OffFor(Duration),
    OffForUnknownDuration,
}

fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    info!("Starting picool control.");
    let args: Vec<String> = env::args().collect();

    cfg_if::cfg_if! {
        if #[cfg(feature = "demo-mode")] {
            let world = DemoWorld::new();
        } else {
            let world = RealWorld::new(
                PathBuf::from(&args[1]),
                args[2].parse().expect("NEED VALIDATION"),
            )
            .expect("NEED VALIDATION");
        }
    }

    let initial_state = init(&world);
    run(initial_state, (0.0,0.0), world);
}

// Pure w.r.t. World
fn init(world: &impl World) -> State {
    match world.restore_power_state() {
        Ok(restored_state) => {
            debug!("Restored state: {}", restored_state);
            match restored_state {
                RestoredPowerState::CurrentlyOn => State::MinimumIntervalOn(world.now()),
                RestoredPowerState::OffFor(duration) => match duration > MINIMUM_OFF_DURATION {
                    true => State::InitiallyOff,
                    false => State::MinimumIntervalOff(world.now() - duration),
                },
                RestoredPowerState::OffForUnknownDuration => State::MinimumIntervalOff(world.now()),
            }
        },
        Err(e) => {
            warn!("Failed get last off transition: {:?}", e);
            State::MinimumIntervalOff(world.now())
        }
    }
}

// Pure w.r.t. World
fn run(initial_state: State, initial_compensation: (f32,f32), mut world: impl World) {
    info!("Initial state: {}", initial_state);
    let mut state = initial_state;

    let (seed_low_compensation, seed_high_compensation) = initial_compensation;
    let mut low_compensator = Compensator::new(TARGET_RANGE.start, seed_low_compensation, MAX_COMPENSATION);
    let mut high_compensator = Compensator::new(TARGET_RANGE.end, seed_high_compensation, -1.0 * MAX_COMPENSATION);

    let mut low_threshold = low_compensator.get_threshold();
    let mut high_threshold = high_compensator.get_threshold();

    let mut extremes = ExtremeTracker::new();
    let mut cycles: u64 = 0;

    loop {
        if state != State::InitiallyOff {
            trace!("Sleeping: {:?}", POLL_DURATION);
            world.sleep(POLL_DURATION);
        }

        let temperature = loop {
            match world.get_temperature() {
                Ok(t) => break t,
                Err(e) => {
                    error!("Could not read temperature. {:?}", e);
                    world.sleep(Duration::from_secs(10));
                    continue;
                }
            }
        };
        trace!("Read temperature: {}", format_c_and_f(temperature));
        extremes.push(temperature);

        let transition_thresholds = low_threshold .. high_threshold;
        let new_state = transition(state, temperature, transition_thresholds, world.now());
        let previous_state = replace(&mut state, new_state);

        if previous_state != new_state {
            info!("State changed: {} -> {}", previous_state, new_state);
        }

        if previous_state.is_on() != new_state.is_on() {
            debug!("Updating power state: {}", new_state.is_on());
            world.set_power_state(new_state.is_on());
            cycles += 1;

            if cycles > 2 {
                if new_state.is_off() {
                    // On -> Off
                    debug!("Persisting last off transition.");
                    if let Err(e) = world.persist_last_off_transition() {
                        warn!("Failed to persist last off transition. {:?}", e);
                    }
    
                    if let Some(max_temp_during_on_cycle) = extremes.max() {
                        trace!("Max temp seen during on cycle: {}", format_c_and_f(max_temp_during_on_cycle));
                        high_compensator.push_observation(max_temp_during_on_cycle);
                        if high_compensator.is_capped() {
                            warn!("Heating compenstation is capped at maximum compensation.");
                        }
                        let old_threshold = replace(&mut high_threshold, high_compensator.get_threshold());
                        if old_threshold != high_threshold {
                            debug!("Updated heating threshold: {} -> {} (target: {})",  format_c_and_f(old_threshold), format_c_and_f(high_threshold), format_c_and_f(TARGET_RANGE.end));
                        }
                    }
                } else {
                    // Off -> On
                    if let Some(min_temp_during_off_cycle) = extremes.min() {
                        trace!("Min temp seen during off cycle: {}", format_c_and_f(min_temp_during_off_cycle));
                        low_compensator.push_observation(min_temp_during_off_cycle);
                        let old_threshold = replace(&mut low_threshold, low_compensator.get_threshold());
                        if low_compensator.is_capped() {
                            warn!("Cooling compenstation is capped at maximum compensation.");
                        }
                        if old_threshold != low_threshold {
                            debug!("Updated cooling threshold: {} -> {} (target: {})",  format_c_and_f(old_threshold), format_c_and_f(low_threshold), format_c_and_f(TARGET_RANGE.start));
                        }
                    }
                }
                extremes.reset();
            }
        }
    }
}

impl State {
    fn is_on(&self) -> bool {
        match self {
            State::InitiallyOff => false,
            State::MinimumIntervalOn(_) => true,
            State::MinimumIntervalOff(_) => false,
            State::On => true,
            State::Off => false,
        }
    }

    fn is_off(&self) -> bool {
        !self.is_on()
    }
}

// Pure
fn transition(initial: State, current_temperature: f32, threshold_range: Range<f32>, now: Instant) -> State {
    match initial {
        State::MinimumIntervalOn(s) if now - s < MINIMUM_ON_DURATION => State::MinimumIntervalOn(s),
        State::MinimumIntervalOff(s) if now - s < MINIMUM_OFF_DURATION => State::MinimumIntervalOff(s),
        State::On | State::MinimumIntervalOn(_) => match is_too_cold(current_temperature, threshold_range.start) {
            true => State::MinimumIntervalOff(now),
            false => State::On,
        },
        State::Off | State::InitiallyOff | State::MinimumIntervalOff(_) => match is_too_hot(current_temperature, threshold_range.end) {
            true => State::MinimumIntervalOn(now),
            false => State::Off,
        },
    }
}

// Pure
fn is_too_cold(temperature: f32, threshold: f32) -> bool {
    temperature < threshold
}

// Pure
fn is_too_hot(temperature: f32, threshold: f32) -> bool {
    temperature > threshold
}

// Pure
fn c_to_f(c: f32) -> f32 {
    (c * 9.0 / 5.0) + 32.0
}

// Pure
fn format_c_and_f(c: f32) -> String {
    format!("{:.2}C {:.2}F", c, c_to_f(c))
}

struct Compensator {
    target: f32,
    observations: VecDeque<f32>,
    compensation: f32,
    max_compensation: f32,
}

impl Compensator {
    pub fn new(target: f32, mut seed_compensation: f32, max_compensation: f32) -> Self {
        if max_compensation.classify() == FpCategory::Zero {
            panic!("max_compensation can not be 0.");
        }
        if seed_compensation.is_nan() {
            error!("Compensator ignoring invalid seed compensation.");
            seed_compensation = 0.0;
        }
        let observations = VecDeque::new();
        Self {
            target,
            observations,
            compensation: seed_compensation,
            max_compensation,
        }
    }

    pub fn get_compensation(&self) -> f32 {
        if self.is_capped() {
            return self.max_compensation;
        }
        if self.is_inverted() {
            return 0.0;
        }
        self.compensation
    }

    pub fn is_capped(&self) -> bool {
        if self.max_compensation < 0.0 {
            self.compensation < self.max_compensation
        } else {
            self.compensation > self.max_compensation
        }
    }

    fn is_inverted(&self) -> bool {
        if self.max_compensation < 0.0 {
            self.compensation > 0.0
        } else {
            self.compensation < 0.0
        }
    }

    pub fn get_threshold(&self) -> f32 {
        self.target + self.get_compensation()
    }
    
    pub fn push_observation(&mut self, value: f32) {
        const MAX_OBSERVATIONS: u8 = 10;
        const MIN_UPDATE: f32 = 0.01;

        if value.classify() == FpCategory::Nan {
            error!("Compensator discarded invalid observation.");
            return;
        }

        let delta = self.get_threshold() - value;
        self.observations.push_back(delta);
        if self.observations.len() > MAX_OBSERVATIONS as usize {
            self.observations.pop_front();
        }
        let mut sorted_observations: Vec<f32> = self.observations.iter().copied().collect();
        sorted_observations.sort_by(|a, b| a.partial_cmp(b).expect("Invariant: Never contains NaN observations."));
        let median_delta = match sorted_observations.len() {
            1 => sorted_observations[0],
            len if len % 2 == 0 => {
                let m1 = sorted_observations[(len / 2) - 1];
                let m2 = sorted_observations[len / 2];
                (m1 + m2) / 2.0
            }
            len => {
                sorted_observations[len / 2]
            }
        };
        let update = median_delta - self.compensation;

        if update.abs() > MIN_UPDATE {
            self.compensation = median_delta;
        }
    }
}

struct ExtremeTracker {
    min: f32,
    max: f32,
    measured: bool,
}

impl ExtremeTracker {
    pub fn new() -> Self {
        Self {
            min: f32::MAX,
            max: f32::MIN,
            measured: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new()
    }

    pub fn push(&mut self, value: f32) {
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.measured = true;
    }

    pub fn min(&self) -> Option<f32> {
        match self.measured {
            true => Some(self.min),
            false => None
        }
    }

    pub fn max(&self) -> Option<f32> {
        match self.measured {
            true => Some(self.max),
            false => None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compensate_default() {
        let compensator = Compensator::new(40.0, 0.0, -3.0);
        assert_eq!(0.0, compensator.get_compensation());
    }

    #[test]
    fn heat_compensate_one_exact_measure() {
        let mut compensator = Compensator::new(40.0, 0.0, -3.0);
        compensator.push_observation(40.0);
        assert_eq!(0.0, compensator.get_compensation());
        assert_eq!(40.0, compensator.get_threshold());
    }

    #[test]
    fn heat_compensate_one_high_measure() {
        let mut compensator = Compensator::new(40.0, 0.0, -3.0);
        compensator.push_observation(41.0);
        assert_eq!(-1.0, compensator.get_compensation());
        assert_eq!(39.0, compensator.get_threshold());
    }

    #[test]
    fn heat_compensate_two_high_measure() {
        let mut compensator = Compensator::new(40.0, 0.0, -3.0);
        compensator.push_observation(42.0);
        assert_eq!(-2.0, compensator.get_compensation());
        assert_eq!(38.0, compensator.get_threshold());
        compensator.push_observation(40.0);
        assert_eq!(-2.0, compensator.get_compensation());
        assert_eq!(38.0, compensator.get_threshold());
    }

    #[test]
    fn heat_compensate_one_high_measure_capped() {
        let mut compensator = Compensator::new(40.0, 0.0, -0.5);
        compensator.push_observation(41.0);
        assert_eq!(-0.5, compensator.get_compensation());
        assert_eq!(39.5, compensator.get_threshold());
        assert_eq!(true, compensator.is_capped());
    }

    #[test]
    fn heat_compensate_one_inverted_measure() {
        let mut compensator = Compensator::new(40.0, 0.0, -0.5);
        compensator.push_observation(39.5);
        assert_eq!(0.0, compensator.get_compensation());
        assert_eq!(40.0, compensator.get_threshold());
        assert_eq!(false, compensator.is_capped());
    }

    #[test]
    fn heat_compensate_one_high_measure_adjust() {
        let mut compensator = Compensator::new(40.0, -1.0, -3.0);
        compensator.push_observation(40.5);
        assert_eq!(-1.5, compensator.get_compensation());
        assert_eq!(38.5, compensator.get_threshold());
    }

    #[test]
    fn heat_compensate_one_low_measure_adjust() {
        let mut compensator = Compensator::new(40.0, -3.0, -3.0);
        compensator.push_observation(39.0);
        assert_eq!(-2.0, compensator.get_compensation());
        assert_eq!(38.0, compensator.get_threshold());
    }

    #[test]
    fn cool_compensate_one_low_measure() {
        let mut compensator = Compensator::new(33.0, 0.0, 3.0);
        compensator.push_observation(32.0);
        assert_eq!(1.0, compensator.get_compensation());
        assert_eq!(34.0, compensator.get_threshold());
    }

    #[test]
    fn cool_compensate_two_low_measure() {
        let mut compensator = Compensator::new(33.0, 0.0, 3.0);
        compensator.push_observation(32.0);
        assert_eq!(1.0, compensator.get_compensation());
        assert_eq!(34.0, compensator.get_threshold());
        compensator.push_observation(33.0);
        assert_eq!(1.0, compensator.get_compensation());
        assert_eq!(34.0, compensator.get_threshold());
    }

    #[test]
    fn cool_compensate_five_low_measure_adjust() {
        let mut compensator = Compensator::new(33.0, 0.0, 3.0);
        // Start 1 true swing
        compensator.push_observation(32.0);
        assert_eq!(1.0, compensator.get_compensation());
        assert_eq!(34.0, compensator.get_threshold());
        compensator.push_observation(33.0);
        assert_eq!(1.0, compensator.get_compensation());
        assert_eq!(34.0, compensator.get_threshold());
        // Swing changes to 1.5
        compensator.push_observation(32.5);
        assert_eq!(1.0, compensator.get_compensation());
        assert_eq!(34.0, compensator.get_threshold());
        compensator.push_observation(32.5);
        assert_eq!(1.25, compensator.get_compensation());
        assert_eq!(34.25, compensator.get_threshold());
        compensator.push_observation(32.75);
        assert_eq!(1.5, compensator.get_compensation());
        assert_eq!(34.5, compensator.get_threshold());
    }

    #[test]
    fn cool_compensate_one_low_measure_capped() {
        let mut compensator = Compensator::new(33.0, 0.0, 0.5);
        compensator.push_observation(32.0);
        assert_eq!(0.5, compensator.get_compensation());
        assert_eq!(33.5, compensator.get_threshold());
        assert_eq!(true, compensator.is_capped());
    }

    #[test]
    fn cool_compensate_one_inverted_measure() {
        let mut compensator = Compensator::new(33.0, 0.0, 0.5);
        compensator.push_observation(33.5);
        assert_eq!(0.0, compensator.get_compensation());
        assert_eq!(33.0, compensator.get_threshold());
        assert_eq!(false, compensator.is_capped());
    }

    #[test]
    fn cool_compensate_one_high_measure_adjust() {
        let mut compensator = Compensator::new(33.0, 3.0, 3.0);
        compensator.push_observation(33.5);
        assert_eq!(2.5, compensator.get_compensation());
        assert_eq!(35.5, compensator.get_threshold());
    }
}