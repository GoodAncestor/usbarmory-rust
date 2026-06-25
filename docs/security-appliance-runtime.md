# Security Appliance Runtime

Portable Rust security-appliance runtime work has moved to
[`GoodAncestor/secure-appliance-rs`](https://github.com/GoodAncestor/secure-appliance-rs).

This repository remains the USB Armory Rust board-support/reference fork:

- i.MX6UL peripheral support
- USB device controller work
- RNG, DCP, eMMC, and image tooling
- USB Armory Mk II hardware examples

The split keeps reusable appliance state machines and SpectrumOS VM targets out
of the older USB Armory fork while still allowing future USB Armory backends to
depend on or integrate with the portable runtime.
