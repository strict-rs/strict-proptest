//-
// Copyright 2023 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! In this example, we're using the state machine testing to test interactions
//! of arbitrary client with an echo server, implemented using `message-io`
//! crate in the `system_under_test` module.

use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::Duration;

use proptest::prelude::*;
use proptest::strict::{TestFailure, TestResult};
use proptest::test_runner::Config;
use proptest_state_machine::{
    ReferenceStateMachine, StateMachineTest, prop_state_machine,
};
use strict_test_support::{ensure, ensure_eq, ensure_ok, ensure_some};

use system_under_test::{
    ClientDialer, Msg, SendStatus, ServerDialer, Transport, init_client,
    init_server, run_client, run_server,
};

// Setup the state machine test using the `prop_state_machine!` macro
prop_state_machine! {
    #![proptest_config(Config {
        // Turn failure persistence off for demonstration. This means that no
        // regression file will be captured.
        failure_persistence: None,
        // Enable verbose mode to make the state machine test print the
        // transitions for each case.
        verbose: 1,
        // Only run 10 cases by default to avoid running out of system resources
        // and taking too long to finish.
        cases: 10,
        .. Config::default()
    })]

    // NOTE: The `#[test]` attribute is commented out in here so we can run it
    // as an example from the `fn main`.

    // #[test]
    fn run_echo_server_test(
        // This is a macro's keyword - only `sequential` is currently supported.
        sequential
        // The number of transitions to be generated for each case. This can
        // be a single numerical value or a range as in here.
        1..20
        // Macro's boilerplate to separate the following identifier.
        =>
        // The name of the type that implements `StateMachineTest`.
        EchoServerTest
    );
}

fn main() -> TestResult {
    // The generated test fn returns the strict verdict; returning it from
    // `main` reports a falsified property through the process exit status
    // instead of a panic.
    run_echo_server_test()
}

/// The reference state of the server and clients.
#[derive(Clone, Debug)]
struct RefState {
    /// The server status.
    is_server_up: bool,
    /// Set of client IDs that are connected.
    clients: HashSet<ClientId>,
    /// We randomly select which transport to use for the test case.
    transport: Transport,
}

/// The possible transitions of the state machine.
#[derive(Clone, Debug)]
enum Transition {
    /// Start the echo server.
    StartServer,
    /// Stop the echo server and disconnect all clients.
    StopServer,
    /// Start the client with the given client ID.
    StartClient(ClientId),
    /// Stop the client with the given client ID.
    StopClient(ClientId),
    /// Send a message from the given client to the server.
    ClientMsg(ClientId, Msg),
}

/// The state of the concrete server and clients under test.
#[derive(Default)]
struct EchoServerTest {
    /// The running server, if the model says it has been started.
    server: Option<TestServer>,
    /// Running clients indexed by their model client IDs.
    clients: HashMap<ClientId, TestClient>,
}

/// Running server resources held by the concrete state machine.
struct TestServer {
    /// A server dialer can be used to send message to clients and to shut-down
    /// the server.
    dialer: ServerDialer,
    /// The a handle of a thread that runs the server listener.
    listener_handle: thread::JoinHandle<()>,
}

/// Running client resources held by the concrete state machine.
struct TestClient {
    /// A client dialer can send messages to the server.
    dialer: ClientDialer,
    /// A handle of a thread that runs the client listener.
    listener_handle: thread::JoinHandle<()>,
    /// Messages received by the listener of the server are forwarded to this
    /// receiver, to be checked by the test.
    msgs_recv: std::sync::mpsc::Receiver<Msg>,
}

/// Stable identifier assigned to a generated client.
type ClientId = usize;

impl ReferenceStateMachine for RefState {
    type State = Self;

    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        prop_oneof![
            Just(Transport::Tcp),
            Just(Transport::FramedTcp),
            Just(Transport::Udp),
            Just(Transport::Ws),
        ]
        .prop_map(|transport| Self {
            is_server_up: false,
            clients: HashSet::default(),
            transport,
        })
        .boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        use Transition::*;
        if state.clients.is_empty() {
            prop_oneof![
                Just(StartServer),
                Just(StopServer),
                (0..32_usize).prop_map(StartClient),
            ]
            .boxed()
        } else {
            let ids: Vec<_> = state.clients.iter().copied().collect();
            let arb_id = proptest::sample::select(ids);
            prop_oneof![
                Just(StartServer),
                Just(StopServer),
                (0..32_usize).prop_map(StartClient),
                arb_id.clone().prop_map(StopClient),
                arb_id.prop_flat_map(|id| arb_msg_from_client()
                    .prop_map(move |msg| { ClientMsg(id, msg) })),
            ]
            .boxed()
        }
    }

    fn apply(
        mut state: Self::State,
        transition: &Self::Transition,
    ) -> Self::State {
        match transition {
            Transition::StartServer => {
                state.is_server_up = true;
            }
            Transition::StopServer => {
                state.is_server_up = false;
                // Any existing clients will be disconnected.
                state.clients = Default::default();
            }
            Transition::StartClient(id) => {
                let _inserted = state.clients.insert(*id);
            }
            Transition::StopClient(id) => {
                let _removed = state.clients.remove(id);
            }
            Transition::ClientMsg(_id, _msg) => {
                // Nothing to do in reference state.
            }
        }
        state
    }

    fn preconditions(
        state: &Self::State,
        transition: &Self::Transition,
    ) -> bool {
        match transition {
            Transition::StartServer => !state.is_server_up,
            Transition::StopServer => state.is_server_up,
            Transition::StartClient(id) => {
                // Only start clients if the server is running and this
                // client ID is not running already.
                state.is_server_up && !state.clients.contains(id)
            }
            Transition::StopClient(id) => {
                // Stop only if this client is actually running.
                state.clients.contains(id)
            }
            Transition::ClientMsg(id, _) => {
                // Can send only if both the server and this client are running.
                state.is_server_up && state.clients.contains(id)
            }
        }
    }
}

/// Generate an arbitrary `Msg` sent by a client.
#[allow(
    clippy::single_call_fn,
    reason = "example strategy generating an arbitrary lowercase alphanumeric client message"
)]
fn arb_msg_from_client() -> impl Strategy<Value = Msg> {
    "[a-z0-9]{1,8}"
}

impl StateMachineTest for EchoServerTest {
    type SystemUnderTest = Self;

    type Reference = RefState;

    fn init_test(
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        Self::default()
    }

    fn apply(
        mut state: Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
        transition: <Self::Reference as ReferenceStateMachine>::Transition,
    ) -> Result<Self::SystemUnderTest, TestFailure> {
        match transition {
            Transition::StartServer => {
                // Assign port dynamically
                let (dialer, listener) = ensure_ok(
                    init_server(ref_state.transport, "127.0.0.1:0"),
                    "the server socket binds and listens",
                )?;

                // Run the listener in a new thread
                let listener_handle =
                    thread::spawn(move || run_server(listener));

                state.server = Some(TestServer {
                    dialer,
                    listener_handle,
                });
            }
            Transition::StopServer => {
                let server = ensure_some(
                    state.server.take(),
                    "stopping the server requires a running server",
                )?;
                server.dialer.handler.stop();

                // Wait for the server listener to stop
                ensure(
                    server.listener_handle.join().is_ok(),
                    "the server listener thread stops cleanly",
                )?;

                if !state.clients.is_empty() {
                    println!(
                        "The server is waiting for all the clients to \
                             stop..."
                    );
                    for (id, client) in std::mem::take(&mut state.clients) {
                        // Ask the client to stop
                        client.dialer.handler.stop();
                        println!("Asking client {id} listener to stop.");
                        // Wait for it to actually stop
                        ensure(
                            client.listener_handle.join().is_ok(),
                            "a client listener thread stops cleanly",
                        )?;
                        println!("Client {id} listener stopped.");
                    }
                    println!("All clients have stopped.");
                }
            }
            Transition::StartClient(id) => {
                // Get the address of the server.
                let server_addr = ensure_some(
                    state.server.as_ref(),
                    "starting a client requires a running server",
                )?
                .dialer
                .address;

                let (listener, dialer) = ensure_ok(
                    init_client(ref_state.transport, server_addr),
                    "the client connects to the server address",
                )?;

                // Open a channel for receiving message from the listener, so
                // that we can check the response the server.
                let (msgs_send, msgs_recv) = std::sync::mpsc::channel();

                let listener_handle = thread::spawn(move || {
                    run_client(listener, |msg| {
                        // The listener thread cannot propagate a TestFailure;
                        // a send error only means the receiver was dropped
                        // because the test case is already over.
                        drop(msgs_send.send(msg));
                    });
                });

                ensure(
                    state
                        .clients
                        .insert(
                            id,
                            TestClient {
                                dialer,
                                listener_handle,
                                msgs_recv,
                            },
                        )
                        .is_none(),
                    "starting a client creates a new concrete client",
                )?;
            }
            Transition::StopClient(id) => {
                // Remove the client
                let client = ensure_some(
                    state.clients.remove(&id),
                    "stopping a client requires it to be running",
                )?;
                // Ask the client to stop
                client.dialer.handler.stop();
                // Wait for it to actually stop
                ensure(
                    client.listener_handle.join().is_ok(),
                    "the stopped client listener thread stops cleanly",
                )?;
            }
            Transition::ClientMsg(id, msg) => {
                let client = ensure_some(
                    state.clients.get_mut(&id),
                    "messaging the server requires the client to be running",
                )?;

                // We use the broken implementation of msg_server, which should
                // be discovered by the test.
                let send_status = system_under_test::msg_server_wrong(
                    &mut client.dialer,
                    &msg,
                );
                ensure(
                    send_status == SendStatus::Sent,
                    "client send reaches the network controller",
                )?;

                // NOTE: To fix the issue found by the state machine, swap
                // `msg_server_wrong` for `msg_server`; the wrong path now
                // reports either a non-`Sent` status or a one-second timeout.
                // system_under_test::msg_server(&mut client.dialer, &msg);

                // Post-condition: The server must send a response back to the
                // client
                println!("Waiting for server response.");
                let recv_msg = ensure_ok(
                    client.msgs_recv.recv_timeout(Duration::from_secs(1)),
                    "the server sends a response back to the client",
                )?;
                ensure_eq(
                    &recv_msg,
                    &msg,
                    "the server echoes the client's message unchanged",
                )?;
            }
        }
        Ok(state)
    }
}

/// Concrete socket-backed echo server used by the example state machine.
mod system_under_test {
    use message_io::network::{Endpoint, NetEvent, ToRemoteAddr};
    pub(crate) use message_io::network::{SendStatus, Transport};
    use message_io::node::{self, NodeEvent, NodeHandler, NodeListener};

    use std::net::{SocketAddr, ToSocketAddrs};

    use std::sync::Arc;
    use std::sync::atomic::{self, AtomicBool};

    /// Atomic ordering used for the client connection flag.
    const ATOMIC_ORDER: atomic::Ordering = atomic::Ordering::SeqCst;

    /// We're only using valid UTF-8 strings here for messages to avoid having
    /// to pull another dev-dependency for serialization.
    pub(crate) type Msg = String;

    /// Listener-side resources for the running echo server.
    pub(crate) struct ServerListener {
        /// Event listener that receives server network events.
        pub listener: NodeListener<()>,
        /// Node handler used to stop the server listener and send replies.
        pub handler: NodeHandler<()>,
    }

    /// Dialer-side resources used by tests to address the running server.
    pub(crate) struct ServerDialer {
        /// Socket address chosen for the server listener.
        pub address: SocketAddr,
        /// Node handler used to stop the server.
        pub handler: NodeHandler<()>,
    }

    /// Listener-side resources for one connected client.
    pub(crate) struct ClientListener {
        /// Local socket address assigned to the client.
        pub address: SocketAddr,
        /// Event listener that receives client network events.
        pub listener: NodeListener<()>,
        /// Server endpoint this client is connected to.
        pub server: Endpoint,
        /// Node handler used to stop the client listener.
        pub handler: NodeHandler<()>,
        /// Server connection status, shared with the [`ClientDialer`].
        pub is_connected: Arc<AtomicBool>,
    }

    /// Dialer-side resources used by tests to send client messages.
    pub(crate) struct ClientDialer {
        /// Server endpoint this client sends messages to.
        pub server: Endpoint,
        /// Node handler used to send messages and stop the client.
        pub handler: NodeHandler<()>,
        /// Server connection status, shared with the [`ClientListener`].
        pub is_connected: Arc<AtomicBool>,
    }

    /// Bind an echo server listener and return the paired dialer resources.
    #[allow(
        clippy::single_call_fn,
        reason = "bind and listen the echo server socket over the message transport"
    )]
    pub(crate) fn init_server(
        transport: Transport,
        addr: impl ToSocketAddrs,
    ) -> std::io::Result<(ServerDialer, ServerListener)> {
        let (handler, listener) = node::split::<()>();

        let (_resource_id, address) =
            handler.network().listen(transport, addr)?;
        println!("Server is running at {address} with {transport}.");

        Ok((
            ServerDialer {
                address,
                handler: handler.clone(),
            },
            ServerListener { listener, handler },
        ))
    }

    /// Run the server event loop, echoing every received message.
    #[allow(
        clippy::single_call_fn,
        reason = "spin the echo server's blocking accept-and-echo loop in the example"
    )]
    pub(crate) fn run_server(listener: ServerListener) {
        let ServerListener { listener, handler } = listener;

        listener.for_each(move |event| match event.network() {
            NetEvent::Connected(_, _) => (), // Only generated at connect() calls.
            NetEvent::Accepted(endpoint, _resource_id) => {
                // Only connection oriented protocols will generate this event
                println!("Client ({}) connected.", endpoint.addr());
            }
            NetEvent::Message(endpoint, msg_bytes) => {
                let message: Msg =
                    String::from_utf8(msg_bytes.to_vec()).unwrap();
                println!("Server received a message \"{message}\".");
                let status = handler.network().send(endpoint, msg_bytes);
                if status != SendStatus::Sent {
                    println!(
                        "Server failed to echo message to {}: {:?}.",
                        endpoint.addr(),
                        status
                    );
                }
            }
            NetEvent::Disconnected(endpoint) => {
                // Only connection oriented protocols will generate this event
                println!("Client ({}) disconnected.", endpoint.addr());
            }
        });
    }

    /// Connect a client listener to the server endpoint.
    #[allow(
        clippy::single_call_fn,
        reason = "connect an example client socket to the echo server endpoint"
    )]
    pub(crate) fn init_client(
        transport: Transport,
        remote_addr: impl ToRemoteAddr,
    ) -> std::io::Result<(ClientListener, ClientDialer)> {
        let (handler, listener) = node::split();
        let (server, address) =
            handler.network().connect(transport, remote_addr)?;

        let is_connected = Arc::new(AtomicBool::new(false));
        Ok((
            ClientListener {
                address,
                server,
                handler: handler.clone(),
                listener,
                is_connected: is_connected.clone(),
            },
            ClientDialer {
                server,
                handler,
                is_connected,
            },
        ))
    }

    /// Run the client event loop and forward received messages to `on_msg`.
    #[allow(
        clippy::single_call_fn,
        reason = "the example client's blocking loop forwarding echoes back"
    )]
    pub(crate) fn run_client(
        listener: ClientListener,
        mut on_msg: impl FnMut(Msg),
    ) {
        let ClientListener {
            address,
            server,
            handler,
            listener,
            is_connected,
        } = listener;

        listener.for_each(move |event| match event {
            NodeEvent::Network(net_event) => match net_event {
                NetEvent::Connected(_, established) => {
                    if established {
                        println!(
                            "Client identified by local port: {}.",
                            address.port()
                        );
                    } else {
                        println!("Cannot connect to server at {server}.");
                    }
                    is_connected.store(established, ATOMIC_ORDER);
                }
                NetEvent::Accepted(_, _) => unreachable!(), // Only generated when a listener accepts
                NetEvent::Message(_, msg_bytes) => {
                    let message: Msg =
                        String::from_utf8(msg_bytes.to_vec()).unwrap();
                    on_msg(message);
                }
                NetEvent::Disconnected(_) => {
                    println!("Server is disconnected.");
                    is_connected.store(false, ATOMIC_ORDER);
                    handler.stop();
                }
            },
            NodeEvent::Signal(()) => {
                // unused
            }
        });
    }

    /// This function will lose messages when they are sent before the client
    /// connection is established.
    #[allow(
        clippy::single_call_fn,
        reason = "intentionally buggy transition handler that sends before the client connection is up"
    )]
    pub(crate) fn msg_server_wrong(
        dialer: &mut ClientDialer,
        msg: &Msg,
    ) -> SendStatus {
        let output_data = msg.as_bytes();

        dialer.handler.network().send(dialer.server, output_data)
    }

    /// Send a message after the client connection has been established.
    #[allow(dead_code)]
    pub(crate) fn msg_server(
        dialer: &mut ClientDialer,
        msg: &Msg,
    ) -> SendStatus {
        let output_data = msg.as_bytes();

        while !dialer.is_connected.load(ATOMIC_ORDER) {
            println!("Waiting for the server to be ready.");
        }

        dialer.handler.network().send(dialer.server, output_data)
    }
}
