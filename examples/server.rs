use std::{sync::mpsc::channel, thread::sleep, time::Duration};

use rust_game_networking::{
    config::ClientServerConfig, initialize, server::Server, set_bindings_log_level, shutdown,
    BindingsLogLevel, PRIVATE_KEY_BYTES,
};

#[path = "./common/mod.rs"]
mod common;
use common::*;

fn main() {
    env_logger::init();

    initialize().unwrap();
    set_bindings_log_level(BindingsLogLevel::Info);

    server_main();

    shutdown();
}

fn server_main() {
    let mut time = 100.0;

    let config = ClientServerConfig::default();
    let max_clients = 16;
    let private_key = [0; PRIVATE_KEY_BYTES];

    let server_address = "127.0.0.1:40000".to_string();
    println!("starting server on address {} (insecure)", &server_address);

    let mut server: Server<TestMessage> = Server::new(&private_key, server_address, config, time);
    server.start(max_clients);

    let (stop_tx, stop_rx) = channel();
    ctrlc::set_handler(move || stop_tx.send(()).unwrap()).expect("Failed to set Ctrl-C handler");
    println!("server started; Ctrl-C to stop");

    let delta_time = 0.01;
    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }
        if !server.running() {
            println!("server not running");
            break;
        }

        time += delta_time;
        server.advance_time(time);
        server.receive_packets();

        for client in 0..max_clients {
            for channel in 0..config.connection.channels.len() {
                if !server.is_client_connected(client) {
                    continue;
                }
                while let Some(message) = server.receive_message(client, channel) {
                    println!(
                        "server got a message from client {} on channel {}:\n\t{:?}",
                        client, channel, &message
                    );
                }
            }
        }

        server.send_packets();

        sleep(Duration::from_secs_f64(delta_time));
    }

    println!("stopping server");
    server.stop();
    println!("server stopped");
}
