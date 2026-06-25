use appliance_core::{
    AlwaysPresent, Appliance, NetworkRx, NetworkTx, Platform, Result, StaticClock, StaticIdentity,
    VolatileStorage,
};
use appliance_example::{FramedCommandAppliance, COMMAND_SLOT};

fn main() {
    let mut platform = HostPlatform::new(&[
        0, 13, b'G', b'E', b'T', b' ', b'/', b'i', b'd', b'e', b'n', b't', b'i', b't', b'y',
    ]);
    let mut app = FramedCommandAppliance::<128, 96, 128, 32>::new();

    app.poll(&mut platform).expect("framed appliance poll");

    println!("{}", hex(platform.network.sent()));
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

struct HostNetwork {
    rx: [u8; 128],
    rx_len: usize,
    tx: [u8; 128],
    tx_len: usize,
}

impl HostNetwork {
    fn new(rx: &[u8]) -> Self {
        let mut network = Self {
            rx: [0; 128],
            rx_len: rx.len(),
            tx: [0; 128],
            tx_len: 0,
        };
        network.rx[..rx.len()].copy_from_slice(rx);
        network
    }

    fn sent(&self) -> &[u8] {
        &self.tx[..self.tx_len]
    }
}

impl NetworkRx for HostNetwork {
    fn recv(&mut self, out: &mut [u8]) -> Result<usize> {
        out[..self.rx_len].copy_from_slice(&self.rx[..self.rx_len]);
        Ok(self.rx_len)
    }
}

impl NetworkTx for HostNetwork {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        self.tx[..frame.len()].copy_from_slice(frame);
        self.tx_len = frame.len();
        Ok(())
    }
}

struct HostPlatform {
    entropy: appliance_core::NullEntropy,
    identity: StaticIdentity,
    storage: VolatileStorage<64>,
    presence: AlwaysPresent,
    clock: StaticClock,
    network: HostNetwork,
}

impl HostPlatform {
    fn new(request: &[u8]) -> Self {
        Self {
            entropy: appliance_core::NullEntropy,
            identity: StaticIdentity {
                model: "host-framed",
                id: &[0xca, 0xfe, 0x01],
            },
            storage: VolatileStorage::new(COMMAND_SLOT),
            presence: AlwaysPresent,
            clock: StaticClock { millis: 0 },
            network: HostNetwork::new(request),
        }
    }
}

impl Platform for HostPlatform {
    type Entropy = appliance_core::NullEntropy;
    type Identity = StaticIdentity;
    type Storage = VolatileStorage<64>;
    type Presence = AlwaysPresent;
    type Clock = StaticClock;
    type Network = HostNetwork;

    fn entropy(&mut self) -> &mut Self::Entropy {
        &mut self.entropy
    }

    fn identity(&self) -> &Self::Identity {
        &self.identity
    }

    fn storage(&mut self) -> &mut Self::Storage {
        &mut self.storage
    }

    fn presence(&mut self) -> &mut Self::Presence {
        &mut self.presence
    }

    fn clock(&self) -> &Self::Clock {
        &self.clock
    }

    fn network(&mut self) -> &mut Self::Network {
        &mut self.network
    }
}
