# Hardware Test

This repository can load examples directly onto a USB armory Mk II in Serial
Downloader mode using the `usd-runner` host tool.

## Prerequisites

- Rust stable with the repository `rust-toolchain.toml` targets installed.
- `flip-lld` installed:

```console
cargo install --git https://github.com/japaric/flip-lld
```

- `usd-runner` installed:

```console
cargo install --path host/usd --force
```

- USB access to the Serial Downloader device.

## Device Detection

Plug in the USB armory Mk II in recovery / Serial Downloader mode and confirm
that the NXP downloader is visible:

```console
lsusb
```

Expected device:

```text
15a2:0080 Freescale Semiconductor, Inc. i.MX 6ULL SystemOnChip in RecoveryMode
```

If enumeration works but `usd-runner` times out, check permissions on the device
node:

```console
ls -l /dev/bus/usb/001/002
```

For a temporary local test, grant write access to the current device node:

```console
sudo chmod a+rw /dev/bus/usb/001/002
```

Adjust the bus and device numbers to match `lsusb`.

For a persistent Linux setup, add a udev rule:

```console
sudo tee /etc/udev/rules.d/50-usbarmory.rules >/dev/null <<'EOF'
ATTRS{idVendor}=="15a2", ATTRS{idProduct}=="0080", TAG+="uaccess", MODE="0666"
EOF
sudo udevadm control --reload-rules
```

Unplug and replug the board after changing udev rules.

## Smoke Test

The current stable Rust build of the `hello` example is expected to work from
DRAM:

```console
cd firmware
COLD_BOOT=1 cargo run --example hello --release --no-default-features --features dram
```

Expected runner result:

```text
(device has reset)
```

The `hello` example writes `Hello, world!` to UART, flushes the serial port, and
then resets the board. Seeing `(device has reset)` confirms that the image was
loaded and control transferred to firmware.

## Serial Output

UART output requires the USB armory debug accessory. The runner looks for:

```text
0403:6011 Future Technology Devices International FT4232H
```

If the debug accessory is not connected, the runner can still load firmware, but
it will print:

```text
serial interface error: USB device 0403:6011 (serial interface) was not found
```

That message does not by itself mean the firmware load failed.

## Current Local Result

Validated locally with Rust 1.96.0:

```console
COLD_BOOT=1 cargo run --example hello --release --no-default-features --features dram
```

Result:

```text
serial interface error: USB device 0403:6011 (serial interface) was not found
(device has reset)
```
