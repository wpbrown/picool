use crate::World;
use anyhow::{anyhow, Context, Result};
use rppal::gpio::{Gpio, OutputPin};
use std::{ffi::OsString, fs, io, path::PathBuf, thread::sleep, time::Duration, time::SystemTime};

const LAST_OFF_TRANSITION_PERSIST_BASE_PATH: &str = "/var/lib/picool";
const LAST_OFF_TRANSITION_PERSIST_FILE_PREFIX: &str = "last_off_";

pub struct RealWorld {
    temperature_sensor_path: PathBuf,
    power_state: OutputPin,
    last_off_persist_path: PathBuf,
}

impl RealWorld {
    pub fn new(temperature_sensor_path: PathBuf, power_state_pin_number: u8) -> Result<Self> {
        let gpio = Gpio::new()?;
        let pin = gpio.get(power_state_pin_number)?.into_output();

        let sensor_name = temperature_sensor_path
            .parent()
            .and_then(|p| p.file_name())
            .context("Invalid temperature path.")?;
        let mut last_off_persist_path = PathBuf::from(LAST_OFF_TRANSITION_PERSIST_BASE_PATH);
        let mut file_name = OsString::from(LAST_OFF_TRANSITION_PERSIST_FILE_PREFIX);
        file_name.push(sensor_name);
        last_off_persist_path.push(file_name);

        Ok(Self {
            temperature_sensor_path,
            power_state: pin,
            last_off_persist_path,
        })
    }
}

impl World for RealWorld {
    fn get_temperature(&self) -> Result<f32> {
        fs::read_to_string(&self.temperature_sensor_path)
            .context("Reading temperature file failed.")
            .and_then(|s| s.parse().context("Parsing temperature value failed."))
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

    fn since_last_off_transition(&self) -> Result<Option<Duration>> {
        let data = fs::read_to_string(&self.last_off_persist_path);
        if let Err(e) = &data {
            if e.kind() == io::ErrorKind::NotFound {
                return Ok(None);
            }
        }
        data.context("Failed reading last off transition storage.")
            .and_then(|d| d.parse().context("Failed parsing stored last off transition."))
            .map(|d| Some(Duration::from_secs(d)))
    }

    fn persist_last_off_transition(&mut self) -> Result<()> {
        let since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Now is never before the epoch.");
        fs::write(&self.last_off_persist_path, since_epoch.as_secs().to_string()).map_err(|e| anyhow!(e))
    }
}
