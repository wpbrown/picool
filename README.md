
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

