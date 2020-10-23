use crate::{RestoredPowerState, World, WorldState};
use anyhow::{anyhow, Context, Result};
use log::warn;
use rppal::gpio::{Gpio, OutputPin};
use std::{
    ffi::OsString,
    fs,
    io::{self, ErrorKind},
    path::PathBuf,
    thread::sleep,
    time::Duration,
    time::Instant,
    time::SystemTime,
};

const PICOOL_PERSIST_BASE_PATH: &str = "/var/lib/picool";
const LAST_OFF_TRANSITION_PERSIST_FILE_PREFIX: &str = "last_off_";
const COMPENSATION_PERSIST_FILE_PREFIX: &str = "comp_";

pub struct RealWorld {
    temperature_sensor_path: PathBuf,
    power_state: OutputPin,
    last_off_persist_path: PathBuf,
    compensation_persist_path: PathBuf,
}

impl RealWorld {
    pub fn new(temperature_sensor_path: PathBuf, power_state_pin_number: u8) -> Result<Self> {
        let gpio = Gpio::new()?;
        let pin = gpio.get(power_state_pin_number)?.into_output();

        let sensor_name = temperature_sensor_path
            .parent()
            .and_then(|p| p.file_name())
            .context("Invalid temperature path.")?;
        let picool_persist_path = PathBuf::from(PICOOL_PERSIST_BASE_PATH);
        let mut last_off_file_name = OsString::from(LAST_OFF_TRANSITION_PERSIST_FILE_PREFIX);
        last_off_file_name.push(sensor_name);
        let mut compensation_file_name = OsString::from(COMPENSATION_PERSIST_FILE_PREFIX);
        compensation_file_name.push(sensor_name);

        Ok(Self {
            temperature_sensor_path,
            power_state: pin,
            last_off_persist_path: picool_persist_path.join(last_off_file_name),
            compensation_persist_path: picool_persist_path.join(compensation_file_name),
        })
    }

    fn restore_power_state(&self) -> Result<RestoredPowerState> {
        if self.power_state.is_set_high() {
            return Ok(RestoredPowerState::CurrentlyOn);
        }
        let data = fs::read_to_string(&self.last_off_persist_path);
        if let Err(e) = &data {
            if e.kind() == io::ErrorKind::NotFound {
                return Ok(RestoredPowerState::OffForUnknownDuration);
            }
        }

        let since_epoch = sec_since_epoch();
        data.context("Failed reading last off transition storage.")
            .and_then(|d| d.parse().context("Failed parsing stored last off transition."))
            .map(|last_transit_sec_since_epoch| {
                RestoredPowerState::OffFor(
                    since_epoch
                        .checked_sub(Duration::from_secs(last_transit_sec_since_epoch))
                        .unwrap_or_default(),
                )
            })
    }
}

impl World for RealWorld {
    fn get_temperature(&self) -> Result<f32> {
        fs::read_to_string(&self.temperature_sensor_path)
            .context("Reading temperature file failed.")
            .and_then(|s| {
                s.trim()
                    .parse::<i32>()
                    .context("Parsing temperature value failed.")
                    .map(|i| i as f32 / 1000.0)
            })
    }

    fn set_power_state(&mut self, state: bool) {
        match state {
            true => self.power_state.set_high(),
            false => self.power_state.set_low(),
        }
    }

    fn sleep(&self, duration: Duration) {
        sleep(duration)
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn restore_state(&self) -> Result<WorldState> {
        self.restore_power_state().map(|power_state| {
            let compensation_data = fs::read_to_string(&self.compensation_persist_path).or_else(|e| match e.kind() {
                ErrorKind::NotFound => Ok(String::from("0.0 0.0")),
                _ => Err(anyhow!(e)),
            });
            let (cooling_compensation, heating_compensation) = compensation_data
                .and_then(|d| {
                    let parts = d.split(' ').collect::<Vec<_>>();
                    match parts.len() {
                        2 => Ok((
                            parts[0].parse().unwrap_or_default(),
                            parts[1].parse().unwrap_or_default(),
                        )),
                        _ => Err(anyhow!("Failed to parse compensation file.")),
                    }
                })
                .or_else(|e| {
                    warn!("Restoring compensation failed: {}", e);
                    Err(e)
                })
                .unwrap_or_default();

            WorldState {
                power_state,
                heating_compensation,
                cooling_compensation,
            }
        })
    }

    fn persist_last_off_transition(&mut self) -> Result<()> {
        let since_epoch = sec_since_epoch();
        fs::write(&self.last_off_persist_path, since_epoch.as_secs().to_string()).map_err(|e| anyhow!(e))
    }

    fn persist_compensation(&mut self, cooling: f32, heating: f32) -> Result<()> {
        fs::write(&self.compensation_persist_path, format!("{} {}", cooling, heating)).map_err(|e| anyhow!(e))
    }
}

fn sec_since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Now is never before the epoch.")
}
