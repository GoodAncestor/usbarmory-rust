#![no_std]

use appliance_core::{Appliance, Clock, Error, Platform, Result, TransportRx, TransportTx};

pub const AGENT_FRAME_HEADER_LEN: usize = 4;
pub const MSG_FAILURE: u8 = 5;
pub const MSG_REQUEST_IDENTITIES: u8 = 11;
pub const MSG_IDENTITIES_ANSWER: u8 = 12;
pub const MSG_SIGN_REQUEST: u8 = 13;
pub const MSG_SIGN_RESPONSE: u8 = 14;

pub trait AgentKey {
    fn public_key_blob<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]>;
    fn comment<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]>;
    fn sign<'a>(&mut self, message: &[u8], flags: u32, out: &'a mut [u8]) -> Result<&'a [u8]>;
}

pub struct AgentStatus {
    pub key_present: bool,
    pub key_source: &'static str,
    pub storage_backend: &'static str,
    pub sign_policy: &'static str,
    pub sign_count: u64,
    pub last_sign_millis: u64,
    pub last_sign_bytes: usize,
    pub last_sign_error: Option<&'static str>,
}

pub struct SshAgentAppliance<
    K,
    const RX: usize = 256,
    const TX: usize = 512,
    const FRAME: usize = 1024,
> {
    key: K,
    status: AgentStatus,
}

impl<K, const RX: usize, const TX: usize, const FRAME: usize> SshAgentAppliance<K, RX, TX, FRAME> {
    pub const fn new(key: K, status: AgentStatus) -> Self {
        Self { key, status }
    }

    pub fn status(&self) -> &AgentStatus {
        &self.status
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn key_mut(&mut self) -> &mut K {
        &mut self.key
    }

    pub fn record_sign_success(&mut self, bytes: usize, monotonic_millis: u64) {
        self.status.sign_count += 1;
        self.status.last_sign_millis = monotonic_millis;
        self.status.last_sign_bytes = bytes;
        self.status.last_sign_error = None;
    }

    pub fn record_sign_failure(&mut self, reason: &'static str) {
        self.status.last_sign_error = Some(reason);
    }
}

impl<K: AgentKey, const RX: usize, const TX: usize, const FRAME: usize>
    SshAgentAppliance<K, RX, TX, FRAME>
{
    pub fn handle_agent_payload<'a>(
        &mut self,
        request: &[u8],
        response: &'a mut [u8],
        monotonic_millis: u64,
    ) -> Result<&'a [u8]> {
        let Some((&message_type, body)) = request.split_first() else {
            self.record_sign_failure("empty request");
            return failure(response);
        };

        match message_type {
            MSG_REQUEST_IDENTITIES => self.identities_answer(response),
            MSG_SIGN_REQUEST => self.sign_response(body, response, monotonic_millis),
            _ => {
                self.record_sign_failure("unsupported message");
                failure(response)
            }
        }
    }

    fn identities_answer<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]> {
        let mut len = 0;
        push(out, &mut len, MSG_IDENTITIES_ANSWER)?;

        if !self.status.key_present {
            write_u32(out, &mut len, 0)?;
            return Ok(&out[..len]);
        }

        let mut key = [0; TX];
        let mut comment = [0; 128];
        let key = self.key.public_key_blob(&mut key)?;
        let comment = self.key.comment(&mut comment)?;

        write_u32(out, &mut len, 1)?;
        write_string(out, &mut len, key)?;
        write_string(out, &mut len, comment)?;
        Ok(&out[..len])
    }

    fn sign_response<'a>(
        &mut self,
        body: &[u8],
        out: &'a mut [u8],
        monotonic_millis: u64,
    ) -> Result<&'a [u8]> {
        if !self.status.key_present {
            self.record_sign_failure("key unavailable");
            return failure(out);
        }

        let Some((key_blob, body)) = read_string(body) else {
            self.record_sign_failure("malformed key blob");
            return failure(out);
        };
        let Some((message, body)) = read_string(body) else {
            self.record_sign_failure("malformed sign data");
            return failure(out);
        };
        if body.len() < 4 {
            self.record_sign_failure("missing sign flags");
            return failure(out);
        }
        let flags = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);

        let mut expected_key = [0; TX];
        let expected_key = self.key.public_key_blob(&mut expected_key)?;
        if expected_key != key_blob {
            self.record_sign_failure("unknown key");
            return failure(out);
        }

        let mut signature = [0; TX];
        let signature = self.key.sign(message, flags, &mut signature)?;

        let mut len = 0;
        let mut wire = [0; TX];
        let mut wire_len = 0;
        write_string(&mut wire, &mut wire_len, b"ssh-ed25519")?;
        write_string(&mut wire, &mut wire_len, signature)?;

        push(out, &mut len, MSG_SIGN_RESPONSE)?;
        write_string(out, &mut len, &wire[..wire_len])?;
        self.record_sign_success(message.len(), monotonic_millis);
        Ok(&out[..len])
    }
}

impl<P: Platform, K: AgentKey, const RX: usize, const TX: usize, const FRAME: usize> Appliance<P>
    for SshAgentAppliance<K, RX, TX, FRAME>
{
    fn poll(&mut self, platform: &mut P) -> Result<()> {
        let mut frame = [0; FRAME];
        let mut request = [0; RX];
        let request = match receive_agent_frame(platform.network(), &mut frame, &mut request) {
            Ok(request) => request,
            Err(Error::NotPresent) => return Ok(()),
            Err(err) => return Err(err),
        };
        let mut response = [0; TX];
        let now = platform.clock().monotonic_millis();
        let response = self.handle_agent_payload(request, &mut response, now)?;

        transmit_agent_frame(platform.network(), &mut frame, response)
    }
}

pub fn receive_agent_frame<'a, T: TransportRx>(
    transport: &mut T,
    frame: &mut [u8],
    payload: &'a mut [u8],
) -> Result<&'a [u8]> {
    let frame_len = transport.receive(frame)?;
    if frame_len < AGENT_FRAME_HEADER_LEN {
        return Err(Error::InvalidInput);
    }

    let payload_len = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    let available = frame_len - AGENT_FRAME_HEADER_LEN;
    if payload_len > available {
        return Err(Error::InvalidInput);
    }
    if payload_len > payload.len() {
        return Err(Error::BufferTooSmall);
    }

    payload[..payload_len]
        .copy_from_slice(&frame[AGENT_FRAME_HEADER_LEN..AGENT_FRAME_HEADER_LEN + payload_len]);
    Ok(&payload[..payload_len])
}

pub fn transmit_agent_frame<T: TransportTx>(
    transport: &mut T,
    frame: &mut [u8],
    payload: &[u8],
) -> Result<()> {
    if payload.len() > u32::MAX as usize {
        return Err(Error::InvalidInput);
    }

    let frame_len = AGENT_FRAME_HEADER_LEN + payload.len();
    if frame_len > frame.len() {
        return Err(Error::BufferTooSmall);
    }

    frame[..AGENT_FRAME_HEADER_LEN].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    frame[AGENT_FRAME_HEADER_LEN..frame_len].copy_from_slice(payload);
    transport.transmit(&frame[..frame_len])
}

fn failure(out: &mut [u8]) -> Result<&[u8]> {
    if out.is_empty() {
        return Err(Error::BufferTooSmall);
    }

    out[0] = MSG_FAILURE;
    Ok(&out[..1])
}

fn push(out: &mut [u8], len: &mut usize, byte: u8) -> Result<()> {
    if *len == out.len() {
        return Err(Error::BufferTooSmall);
    }

    out[*len] = byte;
    *len += 1;
    Ok(())
}

fn write_u32(out: &mut [u8], len: &mut usize, value: u32) -> Result<()> {
    append(out, len, &value.to_be_bytes())
}

fn write_string(out: &mut [u8], len: &mut usize, value: &[u8]) -> Result<()> {
    if value.len() > u32::MAX as usize {
        return Err(Error::InvalidInput);
    }
    write_u32(out, len, value.len() as u32)?;
    append(out, len, value)
}

fn append(out: &mut [u8], len: &mut usize, data: &[u8]) -> Result<()> {
    if out.len() - *len < data.len() {
        return Err(Error::BufferTooSmall);
    }

    out[*len..*len + data.len()].copy_from_slice(data);
    *len += data.len();
    Ok(())
}

fn read_string(input: &[u8]) -> Option<(&[u8], &[u8])> {
    if input.len() < 4 {
        return None;
    }

    let len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if input.len() < 4 + len {
        return None;
    }

    Some((&input[4..4 + len], &input[4 + len..]))
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
    fn answers_identity_request() {
        let mut platform = TestPlatform::new(&agent_frame(&[MSG_REQUEST_IDENTITIES]));
        let mut app = test_app();

        app.poll(&mut platform).unwrap();

        let expected = agent_identities_answer(b"key-blob", b"armory@test");
        assert_eq!(
            platform.network.sent(),
            agent_frame(expected.as_slice()).as_slice()
        );
    }

    #[test]
    fn answers_empty_identity_request_when_key_absent() {
        let mut platform = TestPlatform::new(&agent_frame(&[MSG_REQUEST_IDENTITIES]));
        let mut app = SshAgentAppliance::<TestKey, 128, 256, 512>::new(
            TestKey,
            AgentStatus {
                key_present: false,
                key_source: "none",
                storage_backend: "mmc",
                sign_policy: "allow",
                sign_count: 0,
                last_sign_millis: 0,
                last_sign_bytes: 0,
                last_sign_error: None,
            },
        );

        app.poll(&mut platform).unwrap();

        assert_eq!(
            platform.network.sent(),
            agent_frame(&[MSG_IDENTITIES_ANSWER, 0, 0, 0, 0]).as_slice()
        );
    }

    #[test]
    fn signs_agent_request() {
        let request = agent_sign_request(b"key-blob", b"hello", 0);
        let mut platform = TestPlatform::new(&agent_frame(request.as_slice()));
        let mut app = test_app();
        platform.clock.millis = 42;

        app.poll(&mut platform).unwrap();

        let expected = agent_sign_response(b"sig:hello:0");
        assert_eq!(
            platform.network.sent(),
            agent_frame(expected.as_slice()).as_slice()
        );
        assert_eq!(app.status().sign_count, 1);
        assert_eq!(app.status().last_sign_millis, 42);
        assert_eq!(app.status().last_sign_bytes, 5);
        assert_eq!(app.status().last_sign_error, None);
    }

    #[test]
    fn rejects_unknown_signing_key() {
        let request = agent_sign_request(b"other-key", b"hello", 0);
        let mut platform = TestPlatform::new(&agent_frame(request.as_slice()));
        let mut app = test_app();

        app.poll(&mut platform).unwrap();

        assert_eq!(
            platform.network.sent(),
            agent_frame(&[MSG_FAILURE]).as_slice()
        );
        assert_eq!(app.status().sign_count, 0);
        assert_eq!(app.status().last_sign_error, Some("unknown key"));
    }

    #[test]
    fn rejects_malformed_agent_frame() {
        let mut transport = TestNetwork::new(&[0, 0, 0, 10, MSG_REQUEST_IDENTITIES]);
        let mut frame = [0; 32];
        let mut payload = [0; 8];

        assert_eq!(
            receive_agent_frame(&mut transport, &mut frame, &mut payload),
            Err(Error::InvalidInput)
        );
    }

    #[test]
    fn transmits_agent_frame() {
        let mut transport = TestNetwork::new(&[]);
        let mut frame = [0; 32];

        transmit_agent_frame(&mut transport, &mut frame, &[MSG_FAILURE]).unwrap();

        assert_eq!(transport.sent(), &[0, 0, 0, 1, MSG_FAILURE]);
    }

    fn test_app() -> SshAgentAppliance<TestKey, 128, 256, 512> {
        SshAgentAppliance::new(
            TestKey,
            AgentStatus {
                key_present: true,
                key_source: "storage",
                storage_backend: "mmc",
                sign_policy: "allow",
                sign_count: 0,
                last_sign_millis: 0,
                last_sign_bytes: 0,
                last_sign_error: None,
            },
        )
    }

    struct TestKey;

    impl AgentKey for TestKey {
        fn public_key_blob<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]> {
            copy(out, b"key-blob")
        }

        fn comment<'a>(&self, out: &'a mut [u8]) -> Result<&'a [u8]> {
            copy(out, b"armory@test")
        }

        fn sign<'a>(&mut self, message: &[u8], flags: u32, out: &'a mut [u8]) -> Result<&'a [u8]> {
            let mut len = 0;
            append(out, &mut len, b"sig:")?;
            append(out, &mut len, message)?;
            append(out, &mut len, b":")?;
            append_decimal(out, &mut len, flags)?;
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

    fn append_decimal(out: &mut [u8], len: &mut usize, mut value: u32) -> Result<()> {
        let mut digits = [0; 10];
        let mut n = 0;

        loop {
            digits[n] = b'0' + (value % 10) as u8;
            n += 1;
            value /= 10;
            if value == 0 {
                break;
            }
        }

        for digit in digits[..n].iter().rev() {
            append(out, len, &[*digit])?;
        }

        Ok(())
    }

    fn agent_identities_answer(key: &[u8], comment: &[u8]) -> std::vec::Vec<u8> {
        let mut out = std::vec![MSG_IDENTITIES_ANSWER, 0, 0, 0, 1];
        append_vec_string(&mut out, key);
        append_vec_string(&mut out, comment);
        out
    }

    fn agent_sign_request(key: &[u8], message: &[u8], flags: u32) -> std::vec::Vec<u8> {
        let mut out = std::vec![MSG_SIGN_REQUEST];
        append_vec_string(&mut out, key);
        append_vec_string(&mut out, message);
        out.extend_from_slice(&flags.to_be_bytes());
        out
    }

    fn agent_sign_response(signature: &[u8]) -> std::vec::Vec<u8> {
        let mut wire = std::vec::Vec::new();
        append_vec_string(&mut wire, b"ssh-ed25519");
        append_vec_string(&mut wire, signature);

        let mut out = std::vec![MSG_SIGN_RESPONSE];
        append_vec_string(&mut out, wire.as_slice());
        out
    }

    fn append_vec_string(out: &mut std::vec::Vec<u8>, value: &[u8]) {
        out.extend_from_slice(&(value.len() as u32).to_be_bytes());
        out.extend_from_slice(value);
    }

    fn agent_frame(payload: &[u8]) -> std::vec::Vec<u8> {
        let mut out = std::vec::Vec::new();
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(payload);
        out
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
