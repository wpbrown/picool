use crate::{World, RestoredPowerState, c_to_f};
use anyhow::{anyhow, Context, Result};
use std::{
    cell::Cell, cmp::min, ffi::OsString, fs, io::ErrorKind, path::PathBuf, thread::sleep, time::Duration,
    time::Instant, time::SystemTime,
};

const HEAT_DEGC_PER_SEC: f32 = 0.00163139325;
const COOL_DEGC_PER_SEC: f32 = -0.00106762063;
const TIME_WARP: f32 = 100.0;

pub struct DemoWorld {
    current_temp: Cell<f32>,
    power_state: bool,
}

impl DemoWorld {
    pub fn new() -> Self {
        Self {
            current_temp: Cell::new(4.6),
            power_state: false,
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
    }

    fn sleep(&self, duration: Duration) {
        self.log(&format!("SLEEP: {} sec", duration.as_secs()));
        let mut warped_duration = Duration::from_secs_f32(duration.as_secs_f32() / TIME_WARP);
        while warped_duration.as_millis() != 0 {
            let sleep_for = min(Duration::from_secs(1), warped_duration);
            sleep(sleep_for);
            let change_temp = match self.power_state {
                true => COOL_DEGC_PER_SEC,
                false => HEAT_DEGC_PER_SEC,
            };
            self.current_temp
                .set(self.current_temp.get() + sleep_for.as_secs_f32() * TIME_WARP * change_temp);
            self.log("SLEEPING...");
            warped_duration = warped_duration.checked_sub(sleep_for).unwrap_or_default();
        }
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
