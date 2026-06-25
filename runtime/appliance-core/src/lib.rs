#![cfg_attr(not(feature = "std"), no_std)]

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    BufferTooSmall,
    InvalidInput,
    NotPresent,
    NotSupported,
    Storage,
    Entropy,
    Network,
}

pub trait Entropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<()>;
}

pub trait DeviceIdentity {
    fn stable_id(&self, out: &mut [u8]) -> Result<usize>;
    fn model(&self) -> &'static str;
}

pub trait SealedStorage {
    fn read(&mut self, slot: u32, out: &mut [u8]) -> Result<usize>;
    fn write(&mut self, slot: u32, data: &[u8]) -> Result<()>;
}

pub trait Presence {
    fn asserted(&mut self) -> Result<bool>;
}

pub trait Clock {
    fn monotonic_millis(&self) -> u64;
}

pub trait NetworkRx {
    fn recv(&mut self, out: &mut [u8]) -> Result<usize>;
}

pub trait NetworkTx {
    fn send(&mut self, frame: &[u8]) -> Result<()>;
}

pub trait NetworkDevice: NetworkRx + NetworkTx {}

impl<T> NetworkDevice for T where T: NetworkRx + NetworkTx {}

pub trait TransportRx {
    fn receive(&mut self, out: &mut [u8]) -> Result<usize>;
}

pub trait TransportTx {
    fn transmit(&mut self, message: &[u8]) -> Result<()>;
}

pub trait Transport: TransportRx + TransportTx {}

impl<T> Transport for T where T: TransportRx + TransportTx {}

impl<T> TransportRx for T
where
    T: NetworkRx,
{
    fn receive(&mut self, out: &mut [u8]) -> Result<usize> {
        self.recv(out)
    }
}

impl<T> TransportTx for T
where
    T: NetworkTx,
{
    fn transmit(&mut self, message: &[u8]) -> Result<()> {
        self.send(message)
    }
}

pub trait Platform {
    type Entropy: Entropy;
    type Identity: DeviceIdentity;
    type Storage: SealedStorage;
    type Presence: Presence;
    type Clock: Clock;
    type Network: NetworkDevice;

    fn entropy(&mut self) -> &mut Self::Entropy;
    fn identity(&self) -> &Self::Identity;
    fn storage(&mut self) -> &mut Self::Storage;
    fn presence(&mut self) -> &mut Self::Presence;
    fn clock(&self) -> &Self::Clock;
    fn network(&mut self) -> &mut Self::Network;
}

pub trait Appliance<P: Platform> {
    fn poll(&mut self, platform: &mut P) -> Result<()>;
}

pub struct NullEntropy;

impl Entropy for NullEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        for byte in out {
            *byte = 0;
        }

        Ok(())
    }
}

pub struct StaticIdentity {
    pub model: &'static str,
    pub id: &'static [u8],
}

impl DeviceIdentity for StaticIdentity {
    fn stable_id(&self, out: &mut [u8]) -> Result<usize> {
        if out.len() < self.id.len() {
            return Err(Error::BufferTooSmall);
        }

        out[..self.id.len()].copy_from_slice(self.id);
        Ok(self.id.len())
    }

    fn model(&self) -> &'static str {
        self.model
    }
}

pub struct VolatileStorage<const N: usize> {
    slot: u32,
    len: usize,
    data: [u8; N],
}

impl<const N: usize> VolatileStorage<N> {
    pub const fn new(slot: u32) -> Self {
        Self {
            slot,
            len: 0,
            data: [0; N],
        }
    }
}

impl<const N: usize> SealedStorage for VolatileStorage<N> {
    fn read(&mut self, slot: u32, out: &mut [u8]) -> Result<usize> {
        if slot != self.slot || self.len == 0 {
            return Err(Error::NotPresent);
        }

        if out.len() < self.len {
            return Err(Error::BufferTooSmall);
        }

        out[..self.len].copy_from_slice(&self.data[..self.len]);
        Ok(self.len)
    }

    fn write(&mut self, slot: u32, data: &[u8]) -> Result<()> {
        if slot != self.slot || data.len() > N {
            return Err(Error::InvalidInput);
        }

        self.data[..data.len()].copy_from_slice(data);
        self.len = data.len();
        Ok(())
    }
}

pub struct AlwaysPresent;

impl Presence for AlwaysPresent {
    fn asserted(&mut self) -> Result<bool> {
        Ok(true)
    }
}

pub struct StaticClock {
    pub millis: u64,
}

impl Clock for StaticClock {
    fn monotonic_millis(&self) -> u64 {
        self.millis
    }
}

pub struct NullNetwork;

impl NetworkRx for NullNetwork {
    fn recv(&mut self, _out: &mut [u8]) -> Result<usize> {
        Err(Error::NotPresent)
    }
}

impl NetworkTx for NullNetwork {
    fn send(&mut self, _frame: &[u8]) -> Result<()> {
        Err(Error::NotPresent)
    }
}

pub struct NullPlatform<const STORAGE: usize> {
    pub entropy: NullEntropy,
    pub identity: StaticIdentity,
    pub storage: VolatileStorage<STORAGE>,
    pub presence: AlwaysPresent,
    pub clock: StaticClock,
    pub network: NullNetwork,
}

impl<const STORAGE: usize> NullPlatform<STORAGE> {
    pub const fn new(model: &'static str, id: &'static [u8], slot: u32) -> Self {
        Self {
            entropy: NullEntropy,
            identity: StaticIdentity { model, id },
            storage: VolatileStorage::new(slot),
            presence: AlwaysPresent,
            clock: StaticClock { millis: 0 },
            network: NullNetwork,
        }
    }
}

impl<const STORAGE: usize> Platform for NullPlatform<STORAGE> {
    type Entropy = NullEntropy;
    type Identity = StaticIdentity;
    type Storage = VolatileStorage<STORAGE>;
    type Presence = AlwaysPresent;
    type Clock = StaticClock;
    type Network = NullNetwork;

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
