use crate::{World, RestoredPowerState, c_to_f};
use anyhow::{anyhow, Context, Result};
use std::{
    cell::Cell, cmp::min, ffi::OsString, fs, io::ErrorKind, path::PathBuf, thread::sleep, time::Duration,
    time::Instant, time::SystemTime
};

const HEAT_DEGC_PER_SEC: f32 = 0.00263139325;
const COOL_DEGC_PER_SEC: f32 = -0.00206762063;
const TIME_WARP: f32 = 200.0;
const LATENT_COOL: Duration = Duration::from_secs(300);

pub struct DemoWorld {
    current_temp: Cell<f32>,
    power_state: bool,
    fake_time: Cell<Instant>,
    cycles: u32,
    latent_cooling: Cell<Duration>,
}

impl DemoWorld {
    pub fn new() -> Self {
        Self {
            current_temp: Cell::new(4.6),
            power_state: false,
            fake_time: Cell::new(Instant::now()),
            cycles: 0,
            latent_cooling: Cell::new(Duration::from_secs(0)),
        }
    }

    fn log(&self, message: &str) {
        let power_state = match self.power_state {
            true => "ON",
            false => "OFF",
        };
        println!(
            ">>[{:.2}F][{}] {}",
            c_to_f(self.current_temp.get()),
            power_state,
            message
        );
    }
}

impl World for DemoWorld {
    fn get_temperature(&self) -> Result<f32> {
        self.log("GET_TEMPERATURE");
        Ok(self.current_temp.get())
    }

    fn set_power_state(&mut self, state: bool) {
        self.log(&format!("SET_POWERSTATE: {}", state));
        self.power_state = state;
        if state == false {
            self.cycles += 1;
            if self.cycles == 5 {
                panic!("End of the world.");
            }
            self.latent_cooling.set(LATENT_COOL);
        } else {
            self.latent_cooling.set(Duration::from_secs(0));
        }
    }

    fn sleep(&self, duration: Duration) {
        self.log(&format!("SLEEP: {} sec", duration.as_secs()));
        self.fake_time.set(self.fake_time.get() + duration);
        let change_temp = match self.power_state {
            true => COOL_DEGC_PER_SEC,
            false => HEAT_DEGC_PER_SEC,
        };
        let mut duration = duration;
        if self.latent_cooling.get() > Duration::from_secs(0) {
            let cool_duration = min(duration, self.latent_cooling.get());
            //if duration >= self.latent_cooling.get() {
            self.current_temp.set(self.current_temp.get() + cool_duration.as_secs_f32() * COOL_DEGC_PER_SEC);
            duration -= cool_duration;
            self.latent_cooling.set(self.latent_cooling.get() - cool_duration);
        }
        self.current_temp.set(self.current_temp.get() + duration.as_secs_f32() * change_temp);
    }

    fn now(&self) -> Instant {
        self.fake_time.get()
    }

    fn restore_power_state(&self) -> Result<RestoredPowerState> {
        self.log("GET_SINCE_LAST_OFF");
        Ok(RestoredPowerState::OffForUnknownDuration)
    }

    fn persist_last_off_transition(&mut self) -> Result<()> {
        self.log("PERSIST_LAST_OFF");
        Ok(())
    }
}
