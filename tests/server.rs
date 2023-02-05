use rust_game_networking::{
    config::ClientServerConfig, initialize, server::Server, shutdown, PRIVATE_KEY_BYTES,
};

fn main() {
    initialize();

    server_main();

    shutdown();
}

fn server_main() {
    let time = 100.0;

    let config = ClientServerConfig::default();
    let max_clients = 16;
    let private_key = [0; PRIVATE_KEY_BYTES];

    let mut server = Server::new(&private_key, "127.0.0.1:40000".to_string(), config, time);
    server.start(max_clients);

    // TODO: loop

    server.stop();
}
