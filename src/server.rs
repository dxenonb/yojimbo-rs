use std::ffi::{c_void, CString};
use std::mem::size_of;
use std::ptr::null_mut;
use std::slice;

use crate::channel::ChannelCounters;
use crate::config::ClientServerConfig;
use crate::connection::{Connection, ConnectionErrorLevel};
use crate::message::NetworkMessage;
use crate::network_info::NetworkInfo;
use crate::network_simulator::NetworkSimulator;
use crate::{bindings::*, gf_init_default, PRIVATE_KEY_BYTES};

pub struct Server<M: NetworkMessage> {
    private_key: [u8; PRIVATE_KEY_BYTES],
    address: String,

    /// Base client/server config.
    config: ClientServerConfig,

    /// Current server time in seconds.
    time: f64,

    runtime: *mut ServerRuntime<M>,
}

impl<M: NetworkMessage> Server<M> {
    pub fn new(
        private_key: &[u8; PRIVATE_KEY_BYTES],
        address: String,
        config: ClientServerConfig,
        time: f64,
    ) -> Server<M> {
        assert_ne!(
            size_of::<M>(),
            0,
            "Zero sized message types are not supported"
        );

        Server {
            private_key: *private_key,
            address,
            config,
            time,
            runtime: null_mut(),
        }
    }

    pub fn start(&mut self, max_clients: usize) {
        if !self.runtime.is_null() {
            // TODO: is it better to return an error?
            self.stop();
        }
        self.runtime = ServerRuntime::<M>::new(
            &self.config,
            &self.private_key,
            &self.address,
            max_clients,
            self.time,
        );
    }

    pub fn stop(&mut self) {
        if !self.runtime.is_null() {
            unsafe {
                ServerRuntime::drop(self.runtime);
                self.runtime = null_mut();
            }
        }
    }

    pub fn advance_time(&mut self, new_time: f64) {
        advance_time(self.runtime, new_time)
    }

    pub fn send_packets(&mut self) {
        send_packets(&self.config, self.runtime)
    }

    pub fn receive_packets(&mut self) {
        receive_packets(self.runtime);
    }

    pub fn send_message(&mut self, client_index: usize, channel_index: usize, message: M) {
        unsafe {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.client_connection[client_index].send_message(channel_index, message);
            }
        }
    }

    pub fn receive_message(&mut self, client_index: usize, channel_index: usize) -> Option<M> {
        unsafe {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.client_connection[client_index].receive_message(channel_index)
            } else {
                None
            }
        }
    }

    pub fn client_id(&self, client_index: usize) -> Option<u64> {
        unsafe {
            if let Some(runtime) = self.runtime.as_mut() {
                if is_client_connected(runtime.server, client_index) {
                    Some(netcode_server_client_id(runtime.server, client_index as _))
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    pub fn is_client_connected(&self, client_index: usize) -> bool {
        unsafe {
            if let Some(runtime) = self.runtime.as_mut() {
                is_client_connected(runtime.server, client_index)
            } else {
                false
            }
        }
    }

    pub fn disconnect_client(&mut self, client_index: usize) {
        unsafe {
            if let Some(runtime) = self.runtime.as_mut() {
                if is_client_connected(runtime.server, client_index) {
                    let endpoint = runtime.client_endpoint[client_index];
                    let connection = &mut runtime.client_connection[client_index];
                    disconnect_client(runtime.server, client_index, endpoint, connection);
                    #[allow(clippy::drop_ref)]
                    drop(runtime); // SAFETY: the disconnect callback will fire on `runtime`
                }
            }
        }
    }

    pub fn can_send_message(&self, client_index: usize, channel_index: usize) -> bool {
        unsafe {
            self.runtime
                .as_mut()
                .map(|runtime| {
                    runtime.client_connection[client_index].can_send_message(channel_index)
                })
                .unwrap_or(false)
        }
    }

    pub fn has_messages_to_send(&self, client_index: usize, channel_index: usize) -> bool {
        unsafe {
            self.runtime
                .as_mut()
                .map(|runtime| {
                    runtime.client_connection[client_index].has_messages_to_send(channel_index)
                })
                .unwrap_or(false)
        }
    }

    /// Get the counters for client `client_index` and channel `channel_index`.
    ///
    /// # Panics
    ///
    /// Panics if the server is not running, or one of client and channel_index is out of bounds.
    pub fn channel_counters(&self, client_index: usize, channel_index: usize) -> &ChannelCounters {
        unsafe {
            self.runtime
                .as_mut()
                .map(|runtime| {
                    runtime.client_connection[client_index].channel_counters(channel_index)
                })
                .unwrap()
        }
    }

    pub fn network_simulator_mut(&mut self) -> Option<&mut NetworkSimulator> {
        unsafe {
            self.runtime
                .as_mut()
                .and_then(|runtime| runtime.network_simulator.as_mut())
        }
    }

    // TODO: nice place for doc comments here
    /// Use to configure the network simulator, if one is allocated for this client.
    pub fn with_network_simulator<F: FnOnce(&mut NetworkSimulator)>(&mut self, f: F) {
        if let Some(network_simulator) = self.network_simulator_mut() {
            f(network_simulator)
        }
    }

    /// Take a snapshot of the current network state.
    ///
    /// Returns None if the client is not connected.
    pub fn snapshot_network_info(&self, client_index: usize) -> Option<NetworkInfo> {
        unsafe {
            self.runtime
                .as_ref()
                .and_then(|runtime| runtime.snapshot_network_info(client_index))
        }
    }

    pub fn client_address(&self, client_index: usize) -> Option<NetcodeAddress> {
        if !self.is_client_connected(client_index) {
            return None;
        }
        unsafe {
            let raw = self.runtime.as_ref().map(|runtime| {
                netcode_server_client_address(runtime.server, client_index as i32)
            })?;
            if raw.is_null() {
                None
            } else {
                Some(NetcodeAddress::new(raw))
            }
        }
    }

    pub fn connected_client_count(&self) -> usize {
        let count = unsafe {
            self.runtime
                .as_ref()
                .map(|runtime| netcode_server_num_connected_clients(runtime.server))
                .unwrap_or(0)
        };
        assert!(count >= 0);
        count as usize
    }

    pub fn running(&self) -> bool {
        unsafe { self.runtime.as_ref().is_some() }
    }

    pub fn bound_port(&self) -> Option<u16> {
        unsafe { self.runtime.as_ref().map(|runtime| runtime.bound_port) }
    }
}

impl<M: NetworkMessage> Drop for Server<M> {
    fn drop(&mut self) {
        // this will take care of shutting down the server and freeing `runtime`
        self.stop();
    }
}

struct ServerRuntime<M: NetworkMessage> {
    /// Maximum number of clients supported.
    max_clients: usize,

    /// The netcode server.
    server: *mut netcode_server_t,
    /// The port the netcode server is listening on.
    bound_port: u16,

    /// The network simulator used to simulate packet loss, latency, jitter etc. Optional.
    network_simulator: Option<NetworkSimulator>,

    /// Array of per-client connection classes. This is how messages are exchanged with clients.
    client_connection: Vec<Connection<M>>,
    /// Array of per-client reliable.io endpoints.
    client_endpoint: Vec<*mut reliable_endpoint_t>,

    packet_buffer: Vec<u8>,
}

impl<M: NetworkMessage> ServerRuntime<M> {
    fn new(
        config: &ClientServerConfig,
        private_key: &[u8; PRIVATE_KEY_BYTES],
        address: &str,
        max_clients: usize,
        time: f64,
    ) -> *mut ServerRuntime<M> {
        assert!(max_clients < i32::MAX as usize);

        let network_simulator = config
            .network_simulator
            .as_ref()
            .map(|config| NetworkSimulator::new(config.max_simulator_packets, time));

        let runtime = Box::new(ServerRuntime {
            max_clients,

            server: null_mut(),
            bound_port: 0,

            network_simulator,

            client_connection: Vec::with_capacity(max_clients),
            client_endpoint: Vec::with_capacity(max_clients),

            packet_buffer: vec![0u8; config.connection.max_packet_size],
        });

        /*
            Get a stable pointer which the C callbacks can refer to:

            1 box runtime so it's memory address remains constant
            2 unwrap the box so we can "safely" take pointers to the context
            3 use a raw pointer because we are multiple mutably aliasing `runtime`

            No. 1 is simple: `runtime` can't be on the stack because the
            pointer we hand off would be immediately invalidated when this
            function returns.

            No. 2 is required because *references* to the box's contents are
            invalidated when the box moves (according to the Rust VM), even
            though the physical address remains the same. Run the following in
            miri to verify this is an error:

            ```
            fn main() {
                let x = Box::new(0u32);
                let ptr = &*x as *const u32;
                let _y = x;
                println!("{:?}", unsafe { *ptr });
            }
            ```
        */
        let runtime = Box::into_raw(runtime);

        unsafe {
            for i in 0..max_clients {
                (*runtime)
                    .client_connection
                    .push(Connection::new(config.connection.clone(), time));

                let reliable_config_name = format!("server_endpoint{}", i);
                let mut reliable_config = config.new_reliable_config(
                    runtime.cast(),
                    &reliable_config_name,
                    Some(i),
                    transmit_packet::<M>,
                    process_packet::<M>,
                );

                let endpoint = reliable_endpoint_create(&mut reliable_config, time);
                reliable_endpoint_reset(endpoint);
                (*runtime).client_endpoint.push(endpoint);
            }

            let nc_server = netcode_server(config, private_key, runtime, address, time);

            assert!((*runtime).server.is_null());

            netcode_server_start(nc_server, max_clients as i32);
            (*runtime).bound_port = netcode_server_get_port(nc_server);
            (*runtime).server = nc_server;
        }

        runtime
    }

    /// Stop the server and drop its resources.
    ///
    /// Frees `runtime`.
    ///
    /// # Safety
    ///
    /// `runtime` must be a pointer returned from ServerRuntime::new().
    ///
    /// Specifically:
    ///  - `runtime` pointer must come from a call to Box::into_raw
    ///  - the inner `netcode_server_t` reference must be a valid netcode server
    ///  - the inner `reliable_endpoint_t` references must be valid reliable endpoints
    unsafe fn drop(runtime: *mut ServerRuntime<M>) {
        drop(Box::from_raw(runtime));
    }

    fn snapshot_network_info(&self, client_index: usize) -> Option<NetworkInfo> {
        unsafe {
            assert!(!self.server.is_null());

            if !is_client_connected(self.server, client_index) {
                return None;
            }

            let endpoint = self.client_endpoint[client_index];
            assert!(!endpoint.is_null());

            let mut sent_bandwidth = 0.0;
            let mut received_bandwidth = 0.0;
            let mut acked_bandwidth = 0.0;
            reliable_endpoint_bandwidth(
                endpoint,
                &mut sent_bandwidth,
                &mut received_bandwidth,
                &mut acked_bandwidth,
            );

            let counters = reliable_endpoint_counters(endpoint);
            let num_packets_sent =
                *counters.offset(RELIABLE_ENDPOINT_COUNTER_NUM_PACKETS_SENT as _);
            let num_packets_received =
                *counters.offset(RELIABLE_ENDPOINT_COUNTER_NUM_PACKETS_RECEIVED as _);
            let num_packets_acked =
                *counters.offset(RELIABLE_ENDPOINT_COUNTER_NUM_PACKETS_ACKED as _);

            Some(NetworkInfo {
                rtt: reliable_endpoint_rtt(endpoint),
                packet_loss: reliable_endpoint_packet_loss(endpoint),
                sent_bandwidth,
                received_bandwidth,
                acked_bandwidth,
                num_packets_sent,
                num_packets_received,
                num_packets_acked,
            })
        }
    }

    // TODO: loopback

    unsafe fn transmit_packet(
        &mut self,
        client_index: i32,
        _packet_sequence: u16,
        packet_data: *mut u8,
        packet_bytes: i32,
    ) {
        // TODO: move the unsafety out of connection and handle it here... duh

        if let Some(network_simulator) = &mut self.network_simulator {
            if network_simulator.active() {
                // intercept the packet and defer sending until `advance_time`
                let packet_data =
                    unsafe { slice::from_raw_parts(packet_data, packet_bytes as usize) };
                network_simulator.send_packet(client_index as usize, packet_data);

                return;
            }
        }
        netcode_server_send_packet(self.server, client_index, packet_data, packet_bytes);
    }

    unsafe fn process_packet(
        &mut self,
        client_index: i32,
        packet_sequence: u16,
        packet_data: *mut u8,
        packet_bytes: i32,
    ) -> i32 {
        let connection = &mut self.client_connection[client_index as usize];
        assert!(packet_bytes >= 0);
        let result = connection.process_packet(packet_sequence, packet_data, packet_bytes as usize);
        if result {
            1
        } else {
            0
        }
    }

    fn handle_connect_disconnect(&mut self, client_index: i32, connected: bool) {
        if connected {
            log::debug!("client connected: {}", client_index);
        } else {
            log::debug!("client disconnected: {}", client_index);
            unsafe {
                reliable_endpoint_reset(self.client_endpoint[client_index as usize]);
            }
            self.client_connection[client_index as usize].reset();
            if let Some(network_simulator) = &mut self.network_simulator {
                network_simulator.discard_client_packets(client_index as usize);
            }
        }
    }
}

impl<M: NetworkMessage> Drop for ServerRuntime<M> {
    fn drop(&mut self) {
        unsafe {
            if !self.server.is_null() {
                netcode_server_stop(self.server);
                netcode_server_destroy(self.server);
                self.server = null_mut();
            }

            for endpoint in &mut self.client_endpoint {
                reliable_endpoint_destroy(*endpoint);
                *endpoint = null_mut();
            }
        }
    }
}

fn send_packets<M: NetworkMessage>(config: &ClientServerConfig, runtime: *mut ServerRuntime<M>) {
    if runtime.is_null() {
        return;
    }

    unsafe {
        let nc_server = (*runtime).server;
        assert!(!nc_server.is_null());

        for client_index in 0..(*runtime).client_connection.len() {
            let endpoint = (*runtime).client_endpoint[client_index];

            assert!(!endpoint.is_null());

            if !is_client_connected(nc_server, client_index) {
                continue;
            }

            let packet_sequence = reliable_endpoint_next_packet_sequence(endpoint);

            let written_bytes = {
                // SAFETY: taking references is fine here - no callbacks are firing
                let packet_buffer = &mut (*runtime).packet_buffer;
                let connection = &mut (*runtime).client_connection[client_index];
                assert_eq!(packet_buffer.len(), config.connection.max_packet_size);

                connection.generate_packet(packet_sequence, &mut packet_buffer[..])
            };

            assert!(written_bytes <= config.connection.max_packet_size);

            if written_bytes > 0 {
                // SAFETY: the send_packet causes the transmit_packet to
                // fire, which mutably aliases `runtime`
                reliable_endpoint_send_packet(
                    endpoint,
                    (*runtime).packet_buffer.as_mut_ptr(),
                    written_bytes as i32,
                );
            }
        }
    }
}

fn receive_packets<M: NetworkMessage>(runtime: *mut ServerRuntime<M>) {
    if runtime.is_null() {
        return;
    }

    unsafe {
        assert!(!(*runtime).server.is_null());

        let endpoints = (*runtime).client_endpoint.iter().enumerate();
        let nc_server = (*runtime).server;

        for (client_index, endpoint) in endpoints {
            assert!(!endpoint.is_null());

            loop {
                let mut packet_bytes: i32 = 0;
                let mut packet_sequence: u64 = 0;
                let packet_data = netcode_server_receive_packet(
                    nc_server,
                    client_index as i32,
                    &mut packet_bytes,
                    &mut packet_sequence,
                );

                if packet_data.is_null() {
                    break;
                }

                // SAFETY: receive_packet causes process_packet to fire, which
                // mutably aliases `runtime`
                reliable_endpoint_receive_packet(*endpoint, packet_data, packet_bytes);
                netcode_server_free_packet(nc_server, packet_data.cast());
            }
        }
    }
}

fn advance_time<M: NetworkMessage>(runtime: *mut ServerRuntime<M>, new_time: f64) {
    if runtime.is_null() {
        return;
    }

    unsafe {
        let nc_server = (*runtime).server;
        assert!(!nc_server.is_null());

        netcode_server_update(nc_server, new_time);

        for client_index in 0..(*runtime).max_clients {
            let connection = &mut (*runtime).client_connection[client_index];
            let endpoint = (*runtime).client_endpoint[client_index];

            connection.advance_time(new_time);

            if connection.error_level() != ConnectionErrorLevel::None {
                log::error!(
                    "client {} connection is in error state. disconnecting client",
                    client_index
                );
                disconnect_client(nc_server, client_index, endpoint, connection);
                continue;
            }

            reliable_endpoint_update(endpoint, new_time);
            let mut num_acks = 0;
            let acks = reliable_endpoint_get_acks(endpoint, &mut num_acks);
            connection.process_acks(acks, num_acks);
            reliable_endpoint_clear_acks(endpoint);

            if let Some(network_simulator) = &mut (*runtime).network_simulator {
                network_simulator.advance_time(new_time);
            }
        }

        if let Some(network_simulator) = &mut (*runtime).network_simulator {
            if network_simulator.active() {
                for (client_index, packet_data) in network_simulator.receive_packets() {
                    netcode_server_send_packet(
                        nc_server,
                        client_index as i32,
                        packet_data.as_ptr(),
                        packet_data.len() as i32,
                    );
                }
            }
        }
    }
}

/// Create a netcode server.
///
/// # Panics
///
/// If address contains a null byte.
///
/// # Safety
///
/// `callback_context` must be a valid, non-null server runtime.
unsafe fn netcode_server<M: NetworkMessage>(
    config: &ClientServerConfig,
    private_key: &[u8; PRIVATE_KEY_BYTES],
    callback_context: *mut ServerRuntime<M>,
    address: &str,
    time: f64,
) -> *mut netcode_server_t {
    let mut netcode_config =
        gf_init_default!(netcode_server_config_t, netcode_default_server_config);
    netcode_config.protocol_id = config.protocol_id;
    netcode_config.private_key.copy_from_slice(private_key);

    // do not override `netcode`'s default allocator
    // netcode_config.allocator_context = null_mut();
    // netcode_config.allocate_function = None;
    // netcode_config.free_function = None;

    assert!(!callback_context.is_null());
    netcode_config.callback_context = callback_context.cast();
    netcode_config.connect_disconnect_callback = Some(connect_disconnect_callback::<M>);
    netcode_config.send_loopback_packet_callback = None; // TODO

    let server_address = CString::new(address).unwrap();

    netcode_server_create(server_address.as_ptr() as *mut _, &netcode_config, time)
}

unsafe fn disconnect_client<M>(
    server: *mut netcode_server_t,
    client_index: usize,
    endpoint: *mut reliable_endpoint_t,
    _connection: &mut Connection<M>,
) {
    // TODO: on disconnect, clear send queue https://github.com/networkprotocol/yojimbo/issues/129
    assert!(!server.is_null());
    assert!(!endpoint.is_null());
    netcode_server_disconnect_client(server, client_index as _);
}

unsafe extern "C" fn transmit_packet<M: NetworkMessage>(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) {
    let runtime: *mut ServerRuntime<M> = context.cast();
    runtime
        .as_mut()
        .unwrap()
        .transmit_packet(index, packet_sequence, packet_data, packet_bytes);
}

unsafe extern "C" fn process_packet<M: NetworkMessage>(
    context: *mut c_void,
    index: i32,
    packet_sequence: u16,
    packet_data: *mut u8,
    packet_bytes: i32,
) -> i32 {
    let runtime: *mut ServerRuntime<M> = context.cast();
    runtime
        .as_mut()
        .unwrap()
        .process_packet(index, packet_sequence, packet_data, packet_bytes)
}

unsafe extern "C" fn connect_disconnect_callback<M: NetworkMessage>(
    context: *mut c_void,
    client_index: i32,
    connected: i32,
) {
    let runtime: *mut ServerRuntime<M> = context.cast();
    runtime
        .as_mut()
        .unwrap()
        .handle_connect_disconnect(client_index, connected == 1);
}

unsafe fn is_client_connected(server: *mut netcode_server_t, client_index: usize) -> bool {
    netcode_server_client_connected(server, client_index as i32) != 0
}
