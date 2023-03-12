use std::{thread, time::Duration};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use rust_game_networking::{
    client::Client, config::ClientServerConfig, message::NetworkMessage, server::Server,
    PRIVATE_KEY_BYTES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestMessage {
    value: u64,
}

impl NetworkMessage for TestMessage {
    type Error = std::io::Error;

    fn serialize<W: std::io::Write>(&self, mut writer: W) -> Result<(), Self::Error> {
        writer.write_u64::<LittleEndian>(self.value)?;

        Ok(())
    }

    fn deserialize<R: std::io::Read>(mut reader: R) -> Result<Self, Self::Error> {
        let value = reader.read_u64::<LittleEndian>()?;

        Ok(TestMessage { value })
    }
}

#[test]
fn client_server_messages_multiple_channels() {
    let mut time = 100.0;
    let delta_time = 1.0 / 30.0;
    let seconds_per_segment = 12.0f64;
    let max_segment_iter = (seconds_per_segment / delta_time).ceil() as usize;

    rust_game_networking::initialize().unwrap();

    let mut config = ClientServerConfig::new(2);
    let send_queue_size = 1024;
    for i in 0..2 {
        config.connection.channels[i].sent_packet_buffer_size = 16;
        config.connection.channels[i].message_send_queue_size = send_queue_size;
        config.connection.channels[i].max_messages_per_packet = 8;
    }

    let private_key = [0u8; PRIVATE_KEY_BYTES];

    let client_id = 1234;
    let mut client = Client::new("0.0.0.0".to_string(), config.clone(), time);

    let mut server = Server::new(
        &private_key,
        "127.0.0.1:40000".to_string(),
        config.clone(),
        time,
    );

    server.start(1);

    for _test_repeat in 0..2 {
        assert!(!server.is_client_connected(0));
        assert_eq!(server.connected_client_count(), 0);
        assert!(client.is_disconnected());

        client.insecure_connect(&private_key, client_id, &["127.0.0.1:40000"]);
        assert!(!client.is_connected());
        assert!(!client.is_disconnected());

        for _ in 0..max_segment_iter {
            pump_client_server_update(&mut time, &mut [&mut client], &mut server, delta_time);

            if client.connection_failed() {
                break;
            }

            if client.is_connected() && server.connected_client_count() == 1 {
                break;
            }
        }

        assert!(client.is_connected());
        assert!(!client.is_disconnected());
        assert!(server.is_client_connected(0));
        assert_eq!(server.connected_client_count(), 1);

        for channel in 0..2 {
            let messages_sent = send_queue_size;
            send_messages_from_client(&mut client, channel, messages_sent);
            send_messages_from_server(&mut server, 0, channel, messages_sent);
        }

        let mut client_received = [0, 0];
        let mut server_received = [0, 0];

        for _ in 0..max_segment_iter {
            if !client.is_connected() {
                println!("break because client disconnected");
                break;
            }

            pump_client_server_update(&mut time, &mut [&mut client], &mut server, delta_time);

            for channel in 0..2 {
                receive_messages_from_server(&mut client, channel, &mut client_received[channel]);
                receive_messages_from_client(
                    &mut server,
                    0,
                    channel,
                    &mut server_received[channel],
                );
            }

            let messages_sent = 4 * send_queue_size;
            let received =
                server_received[0] + server_received[1] + client_received[0] + client_received[1];
            if received == messages_sent as u64 {
                println!("break because we hit the goal");
                break;
            }
        }

        assert!(client.is_connected());
        assert!(server.is_client_connected(0));
        assert_eq!(client_received[0], send_queue_size as u64);
        assert_eq!(client_received[1], send_queue_size as u64);
        assert_eq!(server_received[0], send_queue_size as u64);
        assert_eq!(server_received[1], send_queue_size as u64);

        client.disconnect();

        for _ in 0..max_segment_iter {
            pump_client_server_update(&mut time, &mut [&mut client], &mut server, delta_time);
            if !client.is_connected() && !server.is_client_connected(0) {
                break;
            }
        }

        assert_eq!(server.connected_client_count(), 0);
    }

    server.stop();
}

fn send_messages_from_client(client: &mut Client<TestMessage>, channel: usize, count: usize) {
    for i in 0..count {
        if !client.can_send_message(channel) {
            break;
        }

        let value = i as u64;
        let message = TestMessage { value };
        client.send_message(channel, message);
    }
}

fn send_messages_from_server(
    server: &mut Server<TestMessage>,
    client: usize,
    channel: usize,
    count: usize,
) {
    for i in 0..count {
        if !server.can_send_message(client, channel) {
            break;
        }

        let value = i as u64;
        let message = TestMessage { value };
        server.send_message(client, channel, message);
    }
}

fn receive_messages_from_server(
    client: &mut Client<TestMessage>,
    channel: usize,
    expect_value: &mut u64,
) {
    loop {
        let Some(message) = client.receive_message(channel) else { break };

        assert_eq!(message.value, *expect_value);

        *expect_value += 1;
    }
}

fn receive_messages_from_client(
    server: &mut Server<TestMessage>,
    client: usize,
    channel: usize,
    expect_value: &mut u64,
) {
    loop {
        let Some(message) = server.receive_message(client, channel) else { break };

        assert_eq!(message.value, *expect_value);

        *expect_value += 1;
    }
}

fn pump_client_server_update<M: NetworkMessage>(
    time: &mut f64,
    clients: &mut [&mut Client<M>],
    server: &mut Server<M>,
    delta_time: f64,
) {
    for client in clients.iter_mut() {
        client.send_packets();
    }
    server.send_packets();

    for client in clients.iter_mut() {
        client.receive_packets();
    }
    server.receive_packets();

    *time += delta_time;

    for client in clients.iter_mut() {
        client.advance_time(*time);
    }
    server.advance_time(*time);

    thread::sleep(Duration::from_secs_f64(delta_time));
}
