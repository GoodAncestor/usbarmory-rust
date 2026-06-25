#![no_std]

use appliance_core::{Appliance, Error, Platform, Result, TransportRx, TransportTx};

pub trait AgentKey {
    fn public_key<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]>;
    fn fingerprint<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]>;
    fn sign<'a>(&mut self, message: &[u8], out: &'a mut [u8]) -> Result<&'a [u8]>;
}

pub struct AgentStatus {
    pub key_present: bool,
    pub key_source: &'static str,
    pub storage_backend: &'static str,
}

pub struct SshAgentAppliance<K, const RX: usize = 256, const TX: usize = 512> {
    key: K,
    status: AgentStatus,
}

impl<K, const RX: usize, const TX: usize> SshAgentAppliance<K, RX, TX> {
    pub const fn new(key: K, status: AgentStatus) -> Self {
        Self { key, status }
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn key_mut(&mut self) -> &mut K {
        &mut self.key
    }
}

impl<K: AgentKey, const RX: usize, const TX: usize> SshAgentAppliance<K, RX, TX> {
    fn handle_request<'a>(&mut self, request: &[u8], response: &'a mut [u8]) -> Result<&'a [u8]> {
        let request = trim_line(request);

        if request == b"PING" {
            write_response(response, &[b"200 pong\n"])
        } else if request == b"GET /pubkey" {
            let mut key = [0; TX];
            let key = self.key.public_key(&mut key)?;
            write_response(response, &[b"200 ", key, b"\n"])
        } else if request == b"GET /fingerprint" {
            let mut fingerprint = [0; 128];
            let fingerprint = self.key.fingerprint(&mut fingerprint)?;
            write_response(response, &[b"200 ", fingerprint, b"\n"])
        } else if request == b"GET /status" {
            self.write_status(response)
        } else if let Some(message) = request.strip_prefix(b"SIGN ") {
            let mut signature = [0; TX];
            let signature = self.key.sign(message, &mut signature)?;
            write_response(response, &[b"200 ", signature, b"\n"])
        } else {
            write_response(response, &[b"400\n"])
        }
    }

    fn write_status<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]> {
        write_response(
            out,
            &[
                b"200 key_present=",
                if self.status.key_present {
                    b"true".as_slice()
                } else {
                    b"false".as_slice()
                },
                b" key_source=",
                self.status.key_source.as_bytes(),
                b" storage_backend=",
                self.status.storage_backend.as_bytes(),
                b"\n",
            ],
        )
    }
}

impl<P: Platform, K: AgentKey, const RX: usize, const TX: usize> Appliance<P>
    for SshAgentAppliance<K, RX, TX>
{
    fn poll(&mut self, platform: &mut P) -> Result<()> {
        let mut request = [0; RX];
        let request_len = match platform.network().receive(&mut request) {
            Ok(len) => len,
            Err(Error::NotPresent) => return Ok(()),
            Err(err) => return Err(err),
        };
        let mut response = [0; TX];
        let response = self.handle_request(&request[..request_len], &mut response)?;

        platform.network().transmit(response)
    }
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
        AlwaysPresent, Ipv4Address, Ipv4Cidr, LinkState, MacAddress, NetworkConfig, NetworkControl,
        NetworkRx, NetworkTx, NullEntropy, Platform, StaticClock, StaticIdentity, VolatileStorage,
    };

    #[test]
    fn reports_public_key_and_fingerprint() {
        let mut platform = TestPlatform::new(b"GET /pubkey\n");
        let mut app = test_app();

        app.poll(&mut platform).unwrap();
        assert_eq!(platform.network.sent(), b"200 ssh-ed25519 AAAA test@id\n");

        platform.network.set_request(b"GET /fingerprint\n");
        app.poll(&mut platform).unwrap();
        assert_eq!(platform.network.sent(), b"200 SHA256:test\n");
    }

    #[test]
    fn reports_status() {
        let mut platform = TestPlatform::new(b"GET /status\n");
        let mut app = test_app();

        app.poll(&mut platform).unwrap();
        assert_eq!(
            platform.network.sent(),
            b"200 key_present=true key_source=storage storage_backend=mmc\n"
        );
    }

    #[test]
    fn signs_request_data() {
        let mut platform = TestPlatform::new(b"SIGN hello\n");
        let mut app = test_app();

        app.poll(&mut platform).unwrap();
        assert_eq!(platform.network.sent(), b"200 sig:hello\n");
    }

    fn test_app() -> SshAgentAppliance<TestKey, 128, 256> {
        SshAgentAppliance::new(
            TestKey,
            AgentStatus {
                key_present: true,
                key_source: "storage",
                storage_backend: "mmc",
            },
        )
    }

    struct TestKey;

    impl AgentKey for TestKey {
        fn public_key<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]> {
            copy(out, b"ssh-ed25519 AAAA test@id")
        }

        fn fingerprint<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]> {
            copy(out, b"SHA256:test")
        }

        fn sign<'a>(&mut self, message: &[u8], out: &'a mut [u8]) -> Result<&'a [u8]> {
            let mut len = 0;
            append(out, &mut len, b"sig:")?;
            append(out, &mut len, message)?;
            Ok(&out[..len])
        }
    }

    fn copy<'a>(out: &'a mut [u8], data: &[u8]) -> Result<&'a [u8]> {
        if out.len() < data.len() {
            return Err(Error::BufferTooSmall);
        }
        out[..data.len()].copy_from_slice(data);
        Ok(&out[..data.len()])
    }

    fn append(out: &mut [u8], len: &mut usize, data: &[u8]) -> Result<()> {
        if out.len() - *len < data.len() {
            return Err(Error::BufferTooSmall);
        }
        out[*len..*len + data.len()].copy_from_slice(data);
        *len += data.len();
        Ok(())
    }

    struct TestNetwork {
        request: [u8; 128],
        request_len: usize,
        sent: [u8; 256],
        sent_len: usize,
    }

    impl TestNetwork {
        fn new(request: &[u8]) -> Self {
            let mut network = Self {
                request: [0; 128],
                request_len: 0,
                sent: [0; 256],
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

    impl NetworkControl for TestNetwork {
        fn configure(&mut self, _config: NetworkConfig) -> Result<()> {
            Ok(())
        }

        fn config(&self) -> Result<NetworkConfig> {
            Ok(NetworkConfig {
                mac: MacAddress::new([0x1a, 0x55, 0x89, 0xa2, 0x69, 0x42]),
                ipv4: Ipv4Cidr::new(Ipv4Address::new([10, 0, 0, 1]), 24),
                gateway: Some(Ipv4Address::new([10, 0, 0, 2])),
                mtu: 1500,
            })
        }

        fn link_state(&mut self) -> Result<LinkState> {
            Ok(LinkState::Up)
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
                    model: "test",
                    id: b"id",
                },
                storage: VolatileStorage::new(1),
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
