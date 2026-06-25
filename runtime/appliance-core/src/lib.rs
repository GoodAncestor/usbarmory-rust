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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    pub const fn new(octets: [u8; 6]) -> Self {
        Self(octets)
    }

    pub const fn octets(&self) -> [u8; 6] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Address([u8; 4]);

impl Ipv4Address {
    pub const fn new(octets: [u8; 4]) -> Self {
        Self(octets)
    }

    pub const fn octets(&self) -> [u8; 4] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Cidr {
    pub address: Ipv4Address,
    pub prefix_len: u8,
}

impl Ipv4Cidr {
    pub const fn new(address: Ipv4Address, prefix_len: u8) -> Self {
        Self {
            address,
            prefix_len,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkConfig {
    pub mac: MacAddress,
    pub ipv4: Ipv4Cidr,
    pub gateway: Option<Ipv4Address>,
    pub mtu: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkState {
    Down,
    Up,
}

pub trait NetworkControl {
    fn configure(&mut self, config: NetworkConfig) -> Result<()>;
    fn config(&self) -> Result<NetworkConfig>;
    fn link_state(&mut self) -> Result<LinkState>;

    fn poll_link(&mut self) -> Result<()> {
        Ok(())
    }
}

pub trait NetworkRx {
    fn recv(&mut self, out: &mut [u8]) -> Result<usize>;
}

pub trait NetworkTx {
    fn send(&mut self, frame: &[u8]) -> Result<()>;
}

pub trait NetworkDevice: NetworkControl + NetworkRx + NetworkTx {}

impl<T> NetworkDevice for T where T: NetworkControl + NetworkRx + NetworkTx {}

pub trait TransportRx {
    fn receive(&mut self, out: &mut [u8]) -> Result<usize>;
}

pub trait TransportTx {
    fn transmit(&mut self, message: &[u8]) -> Result<()>;
}

pub trait Transport: TransportRx + TransportTx {}

impl<T> Transport for T where T: TransportRx + TransportTx {}

pub const MESSAGE_HEADER_LEN: usize = 2;

pub struct LengthPrefixed<T> {
    inner: T,
}

impl<T> LengthPrefixed<T> {
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: TransportRx> LengthPrefixed<T> {
    pub fn receive_message<'a>(
        &mut self,
        frame: &mut [u8],
        message: &'a mut [u8],
    ) -> Result<&'a [u8]> {
        receive_message(&mut self.inner, frame, message)
    }
}

impl<T: TransportTx> LengthPrefixed<T> {
    pub fn transmit_message(&mut self, frame: &mut [u8], message: &[u8]) -> Result<()> {
        transmit_message(&mut self.inner, frame, message)
    }
}

pub fn receive_message<'a, T: TransportRx>(
    transport: &mut T,
    frame: &mut [u8],
    message: &'a mut [u8],
) -> Result<&'a [u8]> {
    let frame_len = transport.receive(frame)?;

    if frame_len < MESSAGE_HEADER_LEN {
        return Err(Error::InvalidInput);
    }

    let message_len = u16::from_be_bytes([frame[0], frame[1]]) as usize;
    let frame_payload_len = frame_len - MESSAGE_HEADER_LEN;

    if message_len > frame_payload_len {
        return Err(Error::InvalidInput);
    }

    if message_len > message.len() {
        return Err(Error::BufferTooSmall);
    }

    message[..message_len]
        .copy_from_slice(&frame[MESSAGE_HEADER_LEN..MESSAGE_HEADER_LEN + message_len]);
    Ok(&message[..message_len])
}

pub fn transmit_message<T: TransportTx>(
    transport: &mut T,
    frame: &mut [u8],
    message: &[u8],
) -> Result<()> {
    if message.len() > u16::MAX as usize {
        return Err(Error::InvalidInput);
    }

    let frame_len = MESSAGE_HEADER_LEN + message.len();
    if frame_len > frame.len() {
        return Err(Error::BufferTooSmall);
    }

    frame[..MESSAGE_HEADER_LEN].copy_from_slice(&(message.len() as u16).to_be_bytes());
    frame[MESSAGE_HEADER_LEN..frame_len].copy_from_slice(message);
    transport.transmit(&frame[..frame_len])
}

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

impl NetworkControl for NullNetwork {
    fn configure(&mut self, _config: NetworkConfig) -> Result<()> {
        Err(Error::NotSupported)
    }

    fn config(&self) -> Result<NetworkConfig> {
        Err(Error::NotPresent)
    }

    fn link_state(&mut self) -> Result<LinkState> {
        Ok(LinkState::Down)
    }
}

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

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn length_prefixed_receives_message() {
        let mut transport =
            LengthPrefixed::new(TestTransport::new(&[0, 4, b'p', b'i', b'n', b'g']));
        let mut frame = [0; 16];
        let mut message = [0; 8];

        let received = transport.receive_message(&mut frame, &mut message).unwrap();

        assert_eq!(received, b"ping");
    }

    #[test]
    fn length_prefixed_transmits_message() {
        let mut transport = LengthPrefixed::new(TestTransport::new(&[]));
        let mut frame = [0; 16];

        transport.transmit_message(&mut frame, b"pong").unwrap();

        assert_eq!(transport.inner().sent(), &[0, 4, b'p', b'o', b'n', b'g']);
    }

    #[test]
    fn length_prefixed_rejects_short_frame() {
        let mut transport = LengthPrefixed::new(TestTransport::new(&[0]));
        let mut frame = [0; 16];
        let mut message = [0; 8];

        assert_eq!(
            transport.receive_message(&mut frame, &mut message),
            Err(Error::InvalidInput)
        );
    }

    #[test]
    fn length_prefixed_rejects_oversized_message() {
        let mut transport =
            LengthPrefixed::new(TestTransport::new(&[0, 9, b'o', b'v', b'e', b'r']));
        let mut frame = [0; 16];
        let mut message = [0; 8];

        assert_eq!(
            transport.receive_message(&mut frame, &mut message),
            Err(Error::InvalidInput)
        );
    }

    struct TestTransport {
        rx: [u8; 16],
        rx_len: usize,
        tx: [u8; 16],
        tx_len: usize,
    }

    impl TestTransport {
        fn new(rx: &[u8]) -> Self {
            let mut transport = Self {
                rx: [0; 16],
                rx_len: rx.len(),
                tx: [0; 16],
                tx_len: 0,
            };
            transport.rx[..rx.len()].copy_from_slice(rx);
            transport
        }

        fn sent(&self) -> &[u8] {
            &self.tx[..self.tx_len]
        }
    }

    impl TransportRx for TestTransport {
        fn receive(&mut self, out: &mut [u8]) -> Result<usize> {
            if self.rx_len == 0 {
                return Err(Error::NotPresent);
            }

            if self.rx_len > out.len() {
                return Err(Error::BufferTooSmall);
            }

            out[..self.rx_len].copy_from_slice(&self.rx[..self.rx_len]);
            Ok(self.rx_len)
        }
    }

    impl TransportTx for TestTransport {
        fn transmit(&mut self, message: &[u8]) -> Result<()> {
            if message.len() > self.tx.len() {
                return Err(Error::BufferTooSmall);
            }

            self.tx[..message.len()].copy_from_slice(message);
            self.tx_len = message.len();
            Ok(())
        }
    }
}
