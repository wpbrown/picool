
# Setup Pi Zero W Hardware

* [Wire](https://www.circuitbasics.com/raspberry-pi-ds18b20-temperature-sensor-tutorial/) [all](https://lastminuteengineers.com/multiple-ds18b20-arduino-tutorial/) DS18B20 temperature sensors with `DAT` line on `GPIO4`.
* Wire refrigerator relay `VCC` to `GPIO17`.

# Setup OS

* Install Raspberry Pi OS Lite to SD Card.
* In `/boot`:
  * Add empty `ssh` file.
  * Add `wpa_supplicant.conf` with wifi SSID and PSK.
  * Add `dtoverlay=w1-gpio` to `config.txt`.
* Boot Pi and SSH in with default user `pi` password `raspberry`.
* Change password and add authorized ssh key.

# Setup Picool

* Install Telgraf
  * Add a `resource_id` to attach metrics in `telegraf.conf` copy it to `/etc/telegraf`.
  * Fill in `azuremonitor.env` and copy it to `/etc/telegraf`. The service principal needs "Monitoring Metrics Publisher" role on the resource in `resource_id` above.
  * Copy `10-azuremonitor.conf` into new directory `/etc/systemd/system/telegraf.service.d`.
  * Run `sudo systemctl enable telegraf`, `sudo systemctl start telegraf`.
* Install Picool
  * Copy `picool` binary and `influx_temps.sh` into new directory `/opt/picool` and `chmod 755` them.
  * Fill in `picool.env` and copy it into `/etc`.
  * Copy `picool.service` into `/etc/systemd/system`.
  * Run `sudo systemctl daemon-reload`, `sudo systemctl enable picool`, `sudo systemctl start picool`.

# Building for Raspberry Pi Zero

```shell
sudo apt install gcc-arm-linux-gnueabi
rustup target add arm-unknown-linux-gnueabi
cargo build --target arm-unknown-linux-gnueabi --release
```

# Running

Assumes a temperature sensor and a relay for the compressor power is connected.

```shell
# picool <PATH TO TEMPERATURE FILE>                          <RELAY CONTROL GPIO PIN>
./picool "/sys/bus/w1/devices/28-00112233445566/temperature" 17
```

# Demo Mode

Run `cargo run --features demo-mode`. This does not do any actual I/O and simulates the sensor.

