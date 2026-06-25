# Security Appliance Runtime Plan

This repository can become the USB Armory backend for a broader Rust security
appliance runtime, but it should not absorb the whole appliance framework.

Recommended split:

- `usbarmory-rust`: board support, i.MX6UL peripherals, USB device controller,
  eMMC, RNG, DCP, image tooling, hardware examples.
- new runtime/application repo: portable security appliance traits, Spectrum
  Cloud Hypervisor target, shared protocol crates, concrete appliances.

Initial scaffolding now lives under `runtime/`:

- `runtime/appliance-core`: `no_std` platform traits for entropy, identity,
  sealed storage, presence, clock, network configuration/control, network I/O,
  and app-facing transport I/O.
- `runtime/appliance-example`: a tiny host-tested appliance showing how shared
  state-machine code can depend only on those traits. It now includes a minimal
  command appliance over the transport layer.
- `runtime/appliance-ssh-agent`: a `no_std` host-tested SSH-agent state-machine
  scaffold for identity listing, framed OpenSSH agent sign requests, signing
  counters, and last-error status.

Run the current checks with:

```sh
$HOME/bin/nix-portable nix-shell -p cargo rustc rustfmt --run \
  'cargo fmt --all --check && cargo test'
```

## Target Shape

The same Rust appliance core should be able to run on:

- USB Armory Mk II bare metal.
- SpectrumOS Cloud Hypervisor VMs.
- Linux host tests/fuzzers.

The core should depend on platform traits, not on a board or VM directly:

```rust
trait Entropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), Error>;
}

trait SealedStorage {
    fn load(&mut self, slot: SlotId, out: &mut [u8]) -> Result<usize, Error>;
    fn save(&mut self, slot: SlotId, data: &[u8]) -> Result<(), Error>;
    fn wipe(&mut self, slot: SlotId) -> Result<(), Error>;
}

trait DeviceIdentity {
    fn stable_id(&self, out: &mut [u8]) -> Result<usize, Error>;
    fn attestation_quote(&mut self, nonce: &[u8], out: &mut [u8]) -> Result<usize, Error>;
}

trait Presence {
    fn confirm(&mut self, reason: ConfirmReason, timeout_ms: u32) -> Result<bool, Error>;
}
```

Current runtime layering:

- `NetworkControl`: backend-facing MAC, IPv4 CIDR, gateway, MTU, and link-state
  contract for CDC Ethernet, virtio-net, vsock shims, or host-test fakes.
- `NetworkRx`/`NetworkTx`: backend-facing raw frame or packet movement.
- `TransportRx`/`TransportTx`: appliance-facing request/response movement with
  blanket implementations for network devices.
- `Appliance<P>`: polling state machine over a `Platform`, with no allocator
  requirement.

The command example is intentionally small and fixed-buffered. It accepts:

- `PING`
- `GET /identity`
- `GET /network`
- `GET /sealed`
- `PUT /sealed <bytes>`

This is not a final appliance protocol. It is a smoke-test target for USB bulk,
CDC Ethernet, virtio-net, or vsock backends before committing to SSH-agent
framing.

## Target Matrix

| Target | First transport | Current runtime binding | Next concrete task |
| --- | --- | --- | --- |
| Linux host tests | in-memory fake network | command and SSH-agent unit tests | add parser fuzz/property tests |
| USB Armory Mk II | USB CDC-ECM/NCM frames | `NetworkRx`/`NetworkTx` trait target | expose multi-endpoint USB network frames |
| Spectrum VM amd64 | virtio-net or vsock | `TransportRx`/`TransportTx` trait target | boot serial-only Rust kernel, then add virtio transport |
| Spectrum VM aarch64 | virtio-net or vsock | planned trait target | confirm Cloud Hypervisor boot ABI on Asahi host |

## USB Armory Backend

The current blocker is networking, not crypto or storage.

Work sequence:

1. Generalize `firmware/usbarmory/src/usbd.rs` beyond endpoint 0 plus one
   bulk IN/OUT pair.
2. Replace `ENDPTCTRL1`-specific endpoint programming with indexed endpoint
   control register access.
3. Increase dQH/dTD/static buffer pools and make DMA cache maintenance explicit.
4. Implement CDC-ECM or CDC-NCM descriptors and frame movement as a `usb-device`
   class.
5. Implement `NetworkRx`/`NetworkTx` for USB Ethernet frames.
6. Bridge USB Ethernet frames into `smoltcp` with static RX/TX buffers.
7. Run the command appliance over the USB transport as the first smoke test.

Security-specific backend mapping:

- entropy: i.MX RNG.
- sealed storage: eMMC blob plus DCP unique/OTP-key sealing.
- identity: OCOTP UID, later HAB/signed-boot state.
- presence: button/LED confirmation flow.
- audit: fixed eMMC ring buffer or LittleFS partition.

## Spectrum Cloud Hypervisor Backend

This is a new Rust VM target, not an adaptation of the i.MX6UL runtime.

Likely first target:

- `x86_64-unknown-none` or custom JSON target.
- Cloud Hypervisor direct kernel/PVH-style boot.
- serial console first.
- `virtio-drivers` for virtio-net, virtio-rng, virtio-blk/pmem, vsock.
- `smoltcp` over virtio-net or a simpler vsock-first control plane.

Security-specific backend mapping:

- entropy: virtio-rng.
- sealed storage: virtio-blk/pmem plus host-provided key, vTPM, or measured
  launch input.
- identity: measured image hash plus Spectrum VM metadata.
- presence: host/Spectrum-mediated approval, not physical button.
- audit: append-only block or host-collected serial/vsock log.

The local Asahi/Spectrum setup is aarch64. Current TamaGo Cloud Hypervisor
support is amd64-only, and the Rust VM runtime should plan for both amd64 and
aarch64 if it is intended to run locally on Apple Silicon.

## First Appliance

Use the existing Go SSH-agent appliance as the behavioral reference:

- ed25519 key generation.
- sealed private key.
- public key and fingerprint endpoints.
- SSH-agent list/sign messages.
- attestation envelope with nonce and firmware hash.
- reset/wipe path gated by physical or Spectrum-mediated confirmation.

Start with the protocol/state-machine crates and host tests before binding to
either USB or virtio networking.

The first Rust SSH-agent scaffold now models the OpenSSH agent wire shape:

- `SSH_AGENTC_REQUEST_IDENTITIES` (`11`)
- `SSH_AGENT_IDENTITIES_ANSWER` (`12`)
- `SSH_AGENTC_SIGN_REQUEST` (`13`)
- `SSH_AGENT_SIGN_RESPONSE` (`14`)
- 4-byte big-endian OpenSSH agent frame lengths at the appliance boundary.
- status fields for signing policy, sign count, last sign time in monotonic
  milliseconds, last sign byte count, and last signing error.

It deliberately does not yet choose an ed25519 implementation or persistence
backend; those stay behind traits so the same state machine can run on USB
Armory hardware, Spectrum VMs, and host fuzz tests.

The Rust agent can now be tested against actual agent-frame inputs in host
tests. The remaining transport work is backend binding: move bytes between
USB CDC-ECM/NCM, virtio-net, or vsock and the `TransportRx`/`TransportTx`
traits without changing the SSH-agent state machine.

Immediate TODOs:

- Add parser fuzz/property tests for SSH-agent frame lengths, string lengths,
  unsupported message types, and repeated sign failures.
- Add explicit storage wipe and attestation traits before the SSH-agent
  behavior is ported.
- Add a host executable that proxies a Unix `SSH_AUTH_SOCK` into the Rust
  state machine for protocol compatibility testing against `ssh-add`.
- Keep backend work in board/VM crates; keep `appliance-core` free of USB,
  virtio, allocator, and OS assumptions.
