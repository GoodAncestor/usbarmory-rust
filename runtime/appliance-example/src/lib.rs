#![no_std]

use appliance_core::{
    receive_message, transmit_message, Appliance, DeviceIdentity, Error, Platform, Presence,
    Result, SealedStorage, TransportRx, TransportTx,
};

pub const KEY_SLOT: u32 = 1;
pub const COMMAND_SLOT: u32 = 2;

pub struct KeyPresenceAppliance {
    initialized: bool,
}

impl KeyPresenceAppliance {
    pub const fn new() -> Self {
        Self { initialized: false }
    }
}

impl Default for KeyPresenceAppliance {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Platform> Appliance<P> for KeyPresenceAppliance {
    fn poll(&mut self, platform: &mut P) -> Result<()> {
        if self.initialized || !platform.presence().asserted()? {
            return Ok(());
        }

        let seed = b"spectrum-usbarmory-appliance-seed";
        platform.storage().write(KEY_SLOT, seed)?;
        self.initialized = true;
        Ok(())
    }
}

pub struct CommandAppliance<const RX: usize = 128, const TX: usize = 160, const SCRATCH: usize = 64>;

impl<const RX: usize, const TX: usize, const SCRATCH: usize> CommandAppliance<RX, TX, SCRATCH> {
    pub const fn new() -> Self {
        Self
    }

    fn write_response<'a>(out: &'a mut [u8], chunks: &[&[u8]]) -> Result<&'a [u8]> {
        let mut len = 0;

        for chunk in chunks {
            if out.len() - len < chunk.len() {
                return Err(Error::BufferTooSmall);
            }

            out[len..len + chunk.len()].copy_from_slice(chunk);
            len += chunk.len();
        }

        Ok(&out[..len])
    }

    fn write_identity<'a, P: Platform>(platform: &P, out: &'a mut [u8]) -> Result<&'a [u8]> {
        let mut id = [0; SCRATCH];
        let id_len = platform.identity().stable_id(&mut id)?;
        let model = platform.identity().model().as_bytes();
        let mut len = 0;

        append(out, &mut len, b"200 model=")?;
        append(out, &mut len, model)?;
        append(out, &mut len, b" id=")?;
        append_hex(out, &mut len, &id[..id_len])?;
        append(out, &mut len, b"\n")?;

        Ok(&out[..len])
    }

    fn write_sealed<'a, P: Platform>(platform: &mut P, out: &'a mut [u8]) -> Result<&'a [u8]> {
        let mut data = [0; SCRATCH];
        match platform.storage().read(COMMAND_SLOT, &mut data) {
            Ok(len) => Self::write_response(out, &[b"200 ", &data[..len], b"\n"]),
            Err(Error::NotPresent) => Self::write_response(out, &[b"404\n"]),
            Err(err) => Err(err),
        }
    }

    fn store_sealed<P: Platform>(platform: &mut P, data: &[u8]) -> Result<()> {
        if !platform.presence().asserted()? {
            return Err(Error::NotPresent);
        }

        platform.storage().write(COMMAND_SLOT, data)
    }

    fn handle_request<'a, P: Platform>(
        platform: &mut P,
        request: &[u8],
        response: &'a mut [u8],
    ) -> Result<&'a [u8]> {
        if request == b"PING" {
            Self::write_response(response, &[b"200 pong\n"])
        } else if request == b"GET /identity" {
            Self::write_identity(platform, response)
        } else if request == b"GET /sealed" {
            Self::write_sealed(platform, response)
        } else if let Some(data) = request.strip_prefix(b"PUT /sealed ") {
            Self::store_sealed(platform, data)?;
            Self::write_response(response, &[b"204\n"])
        } else {
            Self::write_response(response, &[b"400\n"])
        }
    }
}

impl<const RX: usize, const TX: usize, const SCRATCH: usize> Default
    for CommandAppliance<RX, TX, SCRATCH>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Platform, const RX: usize, const TX: usize, const SCRATCH: usize> Appliance<P>
    for CommandAppliance<RX, TX, SCRATCH>
{
    fn poll(&mut self, platform: &mut P) -> Result<()> {
        let mut request = [0; RX];
        let request_len = match platform.network().receive(&mut request) {
            Ok(len) => len,
            Err(Error::NotPresent) => return Ok(()),
            Err(err) => return Err(err),
        };
        let request = trim_line(&request[..request_len]);
        let mut response = [0; TX];

        let response = Self::handle_request(platform, request, &mut response)?;

        platform.network().transmit(response)
    }
}

pub struct FramedCommandAppliance<
    const FRAME: usize = 192,
    const RX: usize = 128,
    const TX: usize = 160,
    const SCRATCH: usize = 64,
>;

impl<const FRAME: usize, const RX: usize, const TX: usize, const SCRATCH: usize>
    FramedCommandAppliance<FRAME, RX, TX, SCRATCH>
{
    pub const fn new() -> Self {
        Self
    }
}

impl<const FRAME: usize, const RX: usize, const TX: usize, const SCRATCH: usize> Default
    for FramedCommandAppliance<FRAME, RX, TX, SCRATCH>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Platform, const FRAME: usize, const RX: usize, const TX: usize, const SCRATCH: usize>
    Appliance<P> for FramedCommandAppliance<FRAME, RX, TX, SCRATCH>
{
    fn poll(&mut self, platform: &mut P) -> Result<()> {
        let mut frame = [0; FRAME];
        let mut request = [0; RX];
        let request = match receive_message(platform.network(), &mut frame, &mut request) {
            Ok(request) => trim_line(request),
            Err(Error::NotPresent) => return Ok(()),
            Err(err) => return Err(err),
        };
        let mut response = [0; TX];
        let response =
            CommandAppliance::<RX, TX, SCRATCH>::handle_request(platform, request, &mut response)?;

        transmit_message(platform.network(), &mut frame, response)
    }
}

fn append(out: &mut [u8], len: &mut usize, data: &[u8]) -> Result<()> {
    if out.len() - *len < data.len() {
        return Err(Error::BufferTooSmall);
    }

    out[*len..*len + data.len()].copy_from_slice(data);
    *len += data.len();
    Ok(())
}

fn append_hex(out: &mut [u8], len: &mut usize, data: &[u8]) -> Result<()> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    if out.len() - *len < data.len() * 2 {
        return Err(Error::BufferTooSmall);
    }

    for byte in data {
        out[*len] = HEX[(byte >> 4) as usize];
        out[*len + 1] = HEX[(byte & 0x0f) as usize];
        *len += 2;
    }

    Ok(())
}

fn trim_line(mut input: &[u8]) -> &[u8] {
    while matches!(input.last(), Some(b'\n' | b'\r')) {
        input = &input[..input.len() - 1];
    }

    input
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use appliance_core::{
        AlwaysPresent, Appliance, NetworkRx, NetworkTx, NullEntropy, NullPlatform, Platform,
        StaticClock, StaticIdentity, VolatileStorage,
    };

    #[test]
    fn initializes_storage_once_presence_is_asserted() {
        let mut platform = NullPlatform::<64>::new("test", b"id", KEY_SLOT);
        let mut app = KeyPresenceAppliance::new();

        app.poll(&mut platform).unwrap();

        let mut out = [0; 64];
        let n = platform.storage.read(KEY_SLOT, &mut out).unwrap();
        assert_eq!(&out[..n], b"spectrum-usbarmory-appliance-seed");
    }

    #[test]
    fn command_appliance_returns_identity() {
        let mut platform = TestPlatform::new(b"GET /identity\n");
        let mut app = CommandAppliance::<64, 96, 16>::new();

        app.poll(&mut platform).unwrap();

        assert_eq!(
            platform.network.sent(),
            b"200 model=test-appliance id=0102fe\n"
        );
    }

    #[test]
    fn command_appliance_stores_and_reads_sealed_data() {
        let mut platform = TestPlatform::new(b"PUT /sealed secret\n");
        let mut app = CommandAppliance::<64, 96, 16>::new();

        app.poll(&mut platform).unwrap();
        assert_eq!(platform.network.sent(), b"204\n");

        platform.network.set_request(b"GET /sealed\n");
        app.poll(&mut platform).unwrap();
        assert_eq!(platform.network.sent(), b"200 secret\n");
    }

    #[test]
    fn command_appliance_ignores_empty_network_poll() {
        let mut platform = TestPlatform::new(b"");
        let mut app = CommandAppliance::<64, 96, 16>::new();

        app.poll(&mut platform).unwrap();

        assert_eq!(platform.network.sent(), b"");
    }

    #[test]
    fn framed_command_appliance_uses_length_prefix() {
        let mut platform = TestPlatform::new(&[0, 4, b'P', b'I', b'N', b'G']);
        let mut app = FramedCommandAppliance::<64, 64, 96, 16>::new();

        app.poll(&mut platform).unwrap();

        assert_eq!(
            platform.network.sent(),
            &[0, 9, b'2', b'0', b'0', b' ', b'p', b'o', b'n', b'g', b'\n']
        );
    }

    struct TestNetwork {
        request: [u8; 64],
        request_len: usize,
        sent: [u8; 96],
        sent_len: usize,
    }

    impl TestNetwork {
        fn new(request: &[u8]) -> Self {
            let mut network = Self {
                request: [0; 64],
                request_len: 0,
                sent: [0; 96],
                sent_len: 0,
            };
            network.set_request(request);
            network
        }

        fn set_request(&mut self, request: &[u8]) {
            self.request[..request.len()].copy_from_slice(request);
            self.request_len = request.len();
            self.sent_len = 0;
        }

        fn sent(&self) -> &[u8] {
            &self.sent[..self.sent_len]
        }
    }

    impl NetworkRx for TestNetwork {
        fn recv(&mut self, out: &mut [u8]) -> Result<usize> {
            if self.request_len == 0 {
                return Err(Error::NotPresent);
            }

            let len = self.request_len;
            out[..len].copy_from_slice(&self.request[..len]);
            self.request_len = 0;
            Ok(len)
        }
    }

    impl NetworkTx for TestNetwork {
        fn send(&mut self, frame: &[u8]) -> Result<()> {
            self.sent[..frame.len()].copy_from_slice(frame);
            self.sent_len = frame.len();
            Ok(())
        }
    }

    struct TestPlatform {
        entropy: NullEntropy,
        identity: StaticIdentity,
        storage: VolatileStorage<64>,
        presence: AlwaysPresent,
        clock: StaticClock,
        network: TestNetwork,
    }

    impl TestPlatform {
        fn new(request: &[u8]) -> Self {
            Self {
                entropy: NullEntropy,
                identity: StaticIdentity {
                    model: "test-appliance",
                    id: &[0x01, 0x02, 0xfe],
                },
                storage: VolatileStorage::new(COMMAND_SLOT),
                presence: AlwaysPresent,
                clock: StaticClock { millis: 0 },
                network: TestNetwork::new(request),
            }
        }
    }

    impl Platform for TestPlatform {
        type Entropy = NullEntropy;
        type Identity = StaticIdentity;
        type Storage = VolatileStorage<64>;
        type Presence = AlwaysPresent;
        type Clock = StaticClock;
        type Network = TestNetwork;

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
}
