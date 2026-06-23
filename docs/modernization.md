# Modernization Notes

This fork updates the original `usbarmory.rs` codebase for current stable Rust
while keeping the existing crate layout and bare-metal target model.

## Toolchain

- Added `rust-toolchain.toml`.
- Tracks stable Rust.
- Installs `armv7a-none-eabi` and `armv7a-none-eabihf`.
- Validated with Rust 1.96.0.

## Cargo

- Renamed deprecated `.cargo/config` files to `.cargo/config.toml`.
- Removed the historical `cc` git patch from the firmware workspace.
- Refreshed lockfiles under current Cargo.

## Host Tools

The host workspace was updated to current crate major versions where practical:

- `anyhow = "1"`
- `nom = "8"`
- `rusb = "0.9"`
- `serialport = "4.9"`
- `xmas-elf = "0.10"`

The `usd` loader now defaults to the existing `rusb-hid` backend on Linux. This
keeps the default path on `libusb` and avoids requiring `libudev` headers for
the normal loader flow. The alternate `hidapi` backend remains available.

## Firmware

- Added missing generated PAC feature names so modern Cargo check-cfg warnings
  do not fire for declared peripherals.
- Replaced intentionally disabled custom cfgs with `cfg(any())`.
- Updated `static mut` access patterns to use explicit raw pointers where Rust
  2024 compatibility lints now warn about implicit references.
- Cleaned low-risk warnings from unused macro imports and deprecated integer
  constants.

## Examples

The examples crate now exposes explicit memory placement features:

- `ocram` is the default.
- `dram` is available with `--no-default-features --features dram`.

On current stable Rust, small code generation changes can make some examples too
large for OCRAM. The `hello` smoke test builds and runs through the DRAM path:

```console
cd firmware
cargo build --example hello --release --no-default-features --features dram
```

## Verification

The following commands pass locally:

```console
cd common && cargo check --workspace
cd ../host && cargo check --workspace
cd ../firmware && cargo check --workspace
cargo build --example hello --release --no-default-features --features dram
```

Hardware smoke testing is documented in `docs/hardware-test.md`.
