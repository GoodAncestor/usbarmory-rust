# usbarmory-rust

![Apache 2.0 + MIT Licensed][license-image]
![Rust Stable][rust-image]

Support for running bare metal Rust applications, without an operating system,
directly on the [USB armory Mk II][usbarmory] security device.

## Status

This is a modernized fork of the original `usbarmory.rs` codebase. It updates
the project for current stable Rust and validates a USB Serial Downloader
hardware smoke test on a USB armory Mk II.

Check out the [`firmware/usbarmory`](firmware/usbarmory) directory.

For local hardware notes, see [`docs/hardware-test.md`](docs/hardware-test.md).
For a summary of the modernization pass, see
[`docs/modernization.md`](docs/modernization.md).
For the portable Rust/Spectrum security-appliance runtime direction, see
[`docs/security-appliance-runtime.md`](docs/security-appliance-runtime.md) and
the host-tested crates under [`runtime/`](runtime).

## Rust Toolchain

This fork tracks current stable Rust. The repository includes a
`rust-toolchain.toml` that installs the bare-metal USB armory targets:

- `armv7a-none-eabi`
- `armv7a-none-eabihf`

The current modernization pass was validated with Rust **1.96.0**.

## License

Original work copyright © 2020 iqlusion.
Modernization work copyright © GoodAncestor.

Licensed under either of:

 * [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
 * [MIT license](https://opensource.org/license/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you shall be licensed as above,
without any additional terms or conditions.

[//]: # (badges)

[license-image]: https://img.shields.io/badge/license-Apache2.0/MIT-blue.svg
[rust-image]: https://img.shields.io/badge/rust-stable-blue.svg

[//]: # (general links)

[usbarmory]: https://www.crowdsupply.com/f-secure/usb-armory-mk-ii
[Apache License, Version 2.0]: https://www.apache.org/licenses/LICENSE-2.0
[MIT license]: https://opensource.org/license/MIT
