use anyhow::Result;
use std::{env, mem::replace, ops::Range, path::PathBuf, time::Duration};

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
const MINIMUM_ON_DURATION: Duration = Duration::from_secs(60 * 3);
const MINIMUM_OFF_DURATION: Duration = Duration::from_secs(60 * 10);
const POLL_DURATION: Duration = Duration::from_secs(30);

trait World {
    fn get_temperature(&self) -> Result<f32>;
    fn set_power_state(&mut self, state: bool);
    fn sleep(&self, duration: Duration);

    fn since_last_off_transition(&self) -> Result<Option<Duration>>;
    fn persist_last_off_transition(&mut self) -> Result<()>;
}

#[derive(Eq, PartialEq, Copy, Clone)]
enum State {
    InitiallyOff,
    MinimumIntervalOn,
    MinimumIntervalOff,
    On,
    Off,
}

fn main() {
    println!("Starting picool control.");
    let args: Vec<String> = env::args().collect();

    cfg_if::cfg_if! {
        if #[cfg(feature = "demo-mode")] {
            let world = DemoWorld::new();
        } else {
            let world = RealWorld::new(
                PathBuf::from(&args[0]),
                args[1].parse().expect("NEED VALIDATION"),
            )
            .expect("NEED VALIDATION");
        }
    }

    let initial_state = init(&world);
    run(initial_state, world);
}

fn init(world: &impl World) -> State {
    match world.since_last_off_transition() {
        Ok(maybe_duration) => match maybe_duration {
            Some(duration) => match duration > MINIMUM_OFF_DURATION {
                true => State::InitiallyOff,
                false => State::MinimumIntervalOff,
            },
            None => State::InitiallyOff,
        },
        Err(e) => {
            println!("Failed get last off transition: {:?}", e);
            State::MinimumIntervalOff
        }
    }
}

fn run(initial_state: State, mut world: impl World) {
    let mut state = initial_state;

    loop {
        let sleep_duration = match state {
            State::MinimumIntervalOn => Some(MINIMUM_ON_DURATION),
            State::MinimumIntervalOff => Some(MINIMUM_OFF_DURATION),
            State::On => Some(POLL_DURATION),
            State::Off => Some(POLL_DURATION),
            State::InitiallyOff => None,
        };
        if let Some(duration) = sleep_duration {
            world.sleep(duration);
        }

        let temperature = loop {
            match world.get_temperature() {
                Ok(t) => break t,
                Err(e) => {
                    println!("Could not read temperature. {:?}", e);
                    world.sleep(Duration::from_secs(10));
                    continue;
                }
            }
        };

        let new_state = transition(state, temperature);
        let previous_state = replace(&mut state, new_state);

        if previous_state.is_on() != new_state.is_on() {
            world.set_power_state(new_state.is_on());

            if new_state.is_off() {
                if let Err(e) = world.persist_last_off_transition() {
                    println!("Failed to persist last off transition. {:?}", e);
                }
            }
        }
    }
}

impl State {
    fn is_on(&self) -> bool {
        match self {
            State::InitiallyOff => false,
            State::MinimumIntervalOn => true,
            State::MinimumIntervalOff => false,
            State::On => true,
            State::Off => false,
        }
    }

    fn is_off(&self) -> bool {
        !self.is_on()
    }
}

fn transition(initial: State, current_temperature: f32) -> State {
    match initial {
        State::On | State::MinimumIntervalOn => match is_too_cold(current_temperature) {
            true => State::MinimumIntervalOff,
            false => State::On,
        },
        State::Off | State::InitiallyOff | State::MinimumIntervalOff => match is_too_hot(current_temperature) {
            true => State::MinimumIntervalOn,
            false => State::Off,
        },
    }
}

fn is_too_cold(temperature: f32) -> bool {
    temperature < TARGET_RANGE.start
}

fn is_too_hot(temperature: f32) -> bool {
    temperature > TARGET_RANGE.end
}
