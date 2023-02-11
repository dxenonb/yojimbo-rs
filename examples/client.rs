use std::{mem::size_of, sync::mpsc::channel, thread::sleep, time::Duration};

use rust_game_networking::{
    bindings::netcode_random_bytes, client::Client, config::ClientServerConfig, initialize,
    set_bindings_log_level, shutdown, BindingsLogLevel, PRIVATE_KEY_BYTES,
};

#[path = "./common/mod.rs"]
mod common;
use common::*;

fn main() {
    env_logger::init();

    initialize().unwrap();
    set_bindings_log_level(BindingsLogLevel::Error);

    client_main();

    shutdown();
}

fn client_main() {
    let server_address = "127.0.0.1:40000".to_string();

    println!("connecting client (insecure)");

    let mut time = 100.0;

    let mut client_id: u64 = 0;
    unsafe { netcode_random_bytes(&mut client_id as *mut u64 as *mut u8, size_of::<u64>() as _) };

    println!("client id is {:x}", client_id);

    let config = ClientServerConfig::default();
    let mut client: Client<TestMessage> = Client::new("0.0.0.0".to_string(), config, time);

    let private_key = [0; PRIVATE_KEY_BYTES];

    client.insecure_connect(&private_key, client_id, &[&server_address]);

    let (stop_tx, stop_rx) = channel();
    ctrlc::set_handler(move || stop_tx.send(()).unwrap()).expect("Failed to set Ctrl-C handler");

    let client_port = client.bound_port().unwrap();
    println!(
        "client connected to server on {} using port {}; Ctrl-C to stop",
        &server_address, client_port
    );

    let delta_time = 1.0;

    let mut sent = 0;

    loop {
        if stop_rx.try_recv().is_ok() {
            println!("stopping client");
            break;
        }

        if client.is_connected() {
            if time > 20.0 && sent == 0 {
                println!("\tsending first message");
                client.send_message(
                    0,
                    TestMessage::String(TestMessageStruct {
                        value: "hello world!".to_string(),
                        supplmentary_value: 42,
                    }),
                );
                sent += 1;
            } else if time > 40.0 && sent == 1 {
                println!("\tsending second message");
                client.send_message(0, TestMessage::Int(2015));
                sent += 1;
            } else if time > 60.0 && sent == 2 {
                println!("\tsending third message");
                client.send_message(0, TestMessage::Float(3.14159));
                sent += 1;
            }
        }

        client.send_packets();
        client.receive_packets();

        if client.is_disconnected() {
            break;
        }

        time += delta_time;

        client.advance_time(time);

        if client.connection_failed() {
            println!("stopping client");
            break;
        }

        sleep(Duration::from_secs_f64(delta_time));
    }

    client.disconnect();
    println!("client exited");
}
