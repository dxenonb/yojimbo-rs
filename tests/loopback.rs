pub struct LoopbackAdapter {
    client: Client,
    server: Server,
};

impl LoopbackAdapter {
    fn new() -> LoopbackAdapter {
        LoopbackAdapter {
            client,
            server,
        }
    }

    // ClientSendLoopbackPacket
    // ServerSendLoopbackPacket
}

impl Adapter for LoopbackAdapter {
    // CreateMessageFactory?
}

const MAX_CLIENTS: u32 = 1;

fn main() {
    /*
        initialize
     */

    // initialize library
    // set log level

    // TODO: srand( (unsigned int) time( NULL ) );

    /*
        demo
     */

    let mut time = 100.0;

    let config = ClientServerConfig::default();
    let loopback_adapter = LoopbackAdapter::new();

    let private_key = load_private_key();

    let server_port = TODO;
    println!("starting server on port {:?}", &server_port);

    let server_address = TODO(server_port);
    let mut server = Server::new(private_key, server_address, config, loopback_adapter, time);
    
    server.start(MAX_CLIENTS)?;

    println!("started server");

    let client_id = random_u64();
    println!("client id is: {:?}", client_id); // TODO: what is PRIx64 in yojimbo?

    let client_address = DOOT;
    let mut client = Client::new(client_address, config, loopback_adapter, time);

    client.connect_loopback(0, client_id, MAX_CLIENTS);
    server.connect_loopback_client(0, client_id, None);

    // yoinks this wont fly in rust!
    loopback_adapter.client = &client;
    loopback_adapter.server = &server;

    let delta_time = 0.1;

    loop {
        // TODO: handle interupt

        server.send_packets();
        client.send_packets();

        server.receive_packets();
        client.receive_packets();
     
        time += delta_time;

        client.advance_time( time );

        if ( client.is_disconnected() )
            break;

        time += deltaTime;

        server.advance_time( time );

        yojimbo_sleep( deltaTime );
    }

    client.disconnect();
    server.stop();

    /**
     shutdown library
     */
}