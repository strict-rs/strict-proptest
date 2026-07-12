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

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Result as IoResult;
use std::mem::take;
use std::net::ToSocketAddrs;
use std::string::FromUtf8Error;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use message_io::network::SendStatus;
use message_io::network::ToRemoteAddr;
use message_io::network::Transport;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::strict::TestFailure;
use proptest::strict::TestResult;
use proptest::test_runner::Config;
use proptest_state_machine::ReferenceStateMachine;
use proptest_state_machine::StateMachineTest;
use proptest_state_machine::prop_state_machine;
use proptest_state_machine::strict_state_machine_config;
use strict_test_support::ensure;
use strict_test_support::ensure_ok;
use strict_test_support::ensure_some;
use system_under_test::ClientDialer;
use system_under_test::ClientListener;
use system_under_test::Msg;
use system_under_test::ServerDialer;
use system_under_test::ServerListener;

/// Transport operations that construct concrete echo-server resources.
trait EchoTransportExt {
  /// Bind a server listener for this transport.
  fn init_server<A>(self, addr: A) -> IoResult<(ServerDialer, ServerListener)>
  where
    A: ToSocketAddrs;

  /// Connect a client listener to a remote server for this transport.
  fn init_client<A>(self, remote_addr: A) -> IoResult<(ClientListener, ClientDialer)>
  where
    A: ToRemoteAddr;
}

/// Event-loop operation for a running server listener.
trait ServerListenerExt {
  /// Run the blocking server loop and echo every received message.
  fn run_server(self);
}

/// Event-loop operation for a running client listener.
trait ClientListenerExt {
  /// Run the blocking client loop and forward each received message decode
  /// result.
  fn run_client<F>(self, on_msg: F)
  where
    F: FnMut(Result<Msg, FromUtf8Error>);
}

/// Message-sending operation for a running client dialer.
trait ClientDialerExt {
  /// Send a client message using the requested connection-readiness behavior.
  fn msg_server(&mut self, msg: &str, implementation: SendImplementation) -> SendStatus;
}

/// Which send behavior the example should use for client messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SendImplementation {
  /// Send immediately, even if the client connection is not ready yet.
  Immediate,
  /// Wait for the client connection before sending.
  WaitUntilConnected,
}

impl SendImplementation {
  /// The intentionally wrong implementation used by the runnable example.
  const BUGGY_DEFAULT: Self = Self::Immediate;
  /// The corrected implementation that waits for connection readiness.
  const CORRECT: Self = Self::WaitUntilConnected;
  /// All supported send implementations, keeping the teaching alternatives
  /// visible in the example binary.
  const OPTIONS: [Self; 2] = [Self::BUGGY_DEFAULT, Self::CORRECT];
}

// Setup the state machine test using the `prop_state_machine!` macro
prop_state_machine! {
    #![proptest_config(Config {
        // Enable verbose mode to make the state machine test print the
        // transitions for each case.
        verbose: 1,
        // Only run 10 cases by default to avoid running out of system resources
        // and taking too long to finish.
        cases: 10,
        .. strict_state_machine_config()
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
  ensure(
    !SendImplementation::OPTIONS.is_empty(),
    "the echo-server example exposes at least one send implementation",
  )?;
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
  clients:      HashSet<ClientId>,
  /// We randomly select which transport to use for the test case.
  transport:    Transport,
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
  server:  Option<TestServer>,
  /// Running clients indexed by their model client IDs.
  clients: HashMap<ClientId, TestClient>,
}

/// Running server resources held by the concrete state machine.
struct TestServer {
  /// A server dialer can be used to send message to clients and to shut-down
  /// the server.
  dialer:          ServerDialer,
  /// The a handle of a thread that runs the server listener.
  listener_handle: thread::JoinHandle<()>,
}

/// Running client resources held by the concrete state machine.
struct TestClient {
  /// A client dialer can send messages to the server.
  dialer:          ClientDialer,
  /// A handle of a thread that runs the client listener.
  listener_handle: thread::JoinHandle<()>,
  /// Messages received by the listener of the server are forwarded to this
  /// receiver, to be checked by the test.
  msgs_recv:       Receiver<Result<Msg, FromUtf8Error>>,
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
    use Transition::ClientMsg;
    use Transition::StartClient;
    use Transition::StartServer;
    use Transition::StopClient;
    use Transition::StopServer;
    if state.clients.is_empty() {
      prop_oneof![Just(StartServer), Just(StopServer), (0..32_usize).prop_map(StartClient),].boxed()
    } else {
      let ids: Vec<_> = state.clients.iter().copied().collect();
      let arb_id = select(ids);
      prop_oneof![
        Just(StartServer),
        Just(StopServer),
        (0..32_usize).prop_map(StartClient),
        arb_id.clone().prop_map(StopClient),
        arb_id.prop_flat_map(|id| arb_msg_from_client().prop_map(move |msg| { ClientMsg(id, msg) })),
      ]
      .boxed()
    }
  }

  fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
    match *transition {
      Transition::StartServer => {
        state.is_server_up = true;
      }
      Transition::StopServer => {
        state.is_server_up = false;
        // Any existing clients will be disconnected.
        state.clients = HashSet::default();
      }
      Transition::StartClient(id) => {
        let _inserted = state.clients.insert(id);
      }
      Transition::StopClient(id) => {
        let _removed = state.clients.remove(&id);
      }
      Transition::ClientMsg(_id, ref _msg) => {
        // Nothing to do in reference state.
      }
    }
    state
  }

  fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
    match *transition {
      Transition::StartServer => !state.is_server_up,
      Transition::StopServer => state.is_server_up,
      Transition::StartClient(id) => {
        // Only start clients if the server is running and this
        // client ID is not running already.
        state.is_server_up && !state.clients.contains(&id)
      }
      Transition::StopClient(id) => {
        // Stop only if this client is actually running.
        state.clients.contains(&id)
      }
      Transition::ClientMsg(id, _) => {
        // Can send only if both the server and this client are running.
        state.is_server_up && state.clients.contains(&id)
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

impl EchoServerTest {
  /// Start the concrete server for the reference transport.
  fn start_server(&mut self, ref_state: &RefState) -> Result<(), TestFailure> {
    let (dialer, listener) = ensure_ok(
      ref_state.transport.init_server("127.0.0.1:0"),
      "the server socket binds and listens",
    )?;
    let listener_handle = thread::spawn(move || {
      listener.run_server();
    });

    self.server = Some(TestServer {
      dialer,
      listener_handle,
    });
    Ok(())
  }

  /// Stop every concrete client currently tracked by the test state.
  fn stop_all_clients(&mut self) -> Result<(), TestFailure> {
    let mut clients = take(&mut self.clients).into_iter().collect::<Vec<_>>();
    clients.sort_by_key(|entry| entry.0);
    for (_id, client) in clients {
      client.dialer.handler.stop();
      ensure(client.listener_handle.join().is_ok(), "a client listener thread stops cleanly")?;
    }
    Ok(())
  }

  /// Stop the concrete server and any clients it disconnects.
  fn stop_server(&mut self) -> Result<(), TestFailure> {
    let server = ensure_some(self.server.take(), "stopping the server requires a running server")?;
    server.dialer.handler.stop();
    ensure(server.listener_handle.join().is_ok(), "the server listener thread stops cleanly")?;

    if !self.clients.is_empty() {
      self.stop_all_clients()?;
    }
    Ok(())
  }

  /// Start a concrete client connected to the current concrete server.
  fn start_client(&mut self, ref_state: &RefState, id: ClientId) -> Result<(), TestFailure> {
    let server_addr = ensure_some(self.server.as_ref(), "starting a client requires a running server")?
      .dialer
      .address;
    let (listener, dialer) = ensure_ok(
      ref_state.transport.init_client(server_addr),
      "the client connects to the server address",
    )?;
    let (msgs_send, msgs_recv) = mpsc::channel();
    let listener_handle = thread::spawn(move || {
      listener.run_client(|msg| {
        drop(msgs_send.send(msg));
      });
    });

    ensure(
      self
        .clients
        .insert(id, TestClient {
          dialer,
          listener_handle,
          msgs_recv,
        })
        .is_none(),
      "starting a client creates a new concrete client",
    )
  }

  /// Stop one concrete client by model identifier.
  fn stop_client(&mut self, id: ClientId) -> Result<(), TestFailure> {
    let client = ensure_some(self.clients.remove(&id), "stopping a client requires it to be running")?;
    client.dialer.handler.stop();
    ensure(
      client.listener_handle.join().is_ok(),
      "the stopped client listener thread stops cleanly",
    )
  }

  /// Send a message from a concrete client and verify the server echo.
  fn send_client_message(&mut self, id: ClientId, msg: &str) -> Result<(), TestFailure> {
    let client = ensure_some(self.clients.get_mut(&id), "messaging the server requires the client to be running")?;
    let send_status = client.dialer.msg_server(msg, SendImplementation::BUGGY_DEFAULT);
    ensure(send_status == SendStatus::Sent, "client send reaches the network controller")?;

    // NOTE: To fix the issue found by the state machine, swap the send
    // implementation from `BUGGY_DEFAULT` to `CORRECT`; the wrong path
    // now reports either a non-`Sent` status or a one-second timeout.
    let received = ensure_ok(
      client.msgs_recv.recv_timeout(Duration::from_secs(1)),
      "the server sends a response back to the client",
    )?;
    let recv_msg = ensure_ok(received, "the server response is valid UTF-8")?;
    ensure(recv_msg == msg, "the server echoes the client's message unchanged")
  }
}

impl StateMachineTest for EchoServerTest {
  type SystemUnderTest = Self;

  type Reference = RefState;

  fn init_test(_ref_state: &<Self::Reference as ReferenceStateMachine>::State) -> Self::SystemUnderTest {
    Self::default()
  }

  fn apply(
    mut state: Self::SystemUnderTest,
    ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    transition: <Self::Reference as ReferenceStateMachine>::Transition,
  ) -> Result<Self::SystemUnderTest, TestFailure> {
    match transition {
      Transition::StartServer => {
        state.start_server(ref_state)?;
      }
      Transition::StopServer => {
        state.stop_server()?;
      }
      Transition::StartClient(id) => {
        state.start_client(ref_state, id)?;
      }
      Transition::StopClient(id) => {
        state.stop_client(id)?;
      }
      Transition::ClientMsg(id, msg) => {
        state.send_client_message(id, &msg)?;
      }
    }
    Ok(state)
  }
}

/// Concrete socket-backed echo server used by the example state machine.
pub mod system_under_test {
  use std::fmt;
  use std::io::Result as IoResult;
  use std::net::SocketAddr;
  use std::net::ToSocketAddrs;
  use std::string::FromUtf8Error;
  use std::sync::Arc;
  use std::sync::atomic;
  use std::sync::atomic::AtomicBool;
  use std::thread::yield_now;

  use message_io::network::Endpoint;
  use message_io::network::NetEvent;
  use message_io::network::SendStatus;
  use message_io::network::ToRemoteAddr;
  use message_io::network::Transport;
  use message_io::node;
  use message_io::node::NodeEvent;
  use message_io::node::NodeHandler;
  use message_io::node::NodeListener;

  use super::ClientDialerExt;
  use super::ClientListenerExt;
  use super::EchoTransportExt;
  use super::SendImplementation;
  use super::ServerListenerExt;

  /// Atomic ordering used for the client connection flag.
  const ATOMIC_ORDER: atomic::Ordering = atomic::Ordering::SeqCst;

  /// Client messages are generated as UTF-8 strings; the listener still
  /// reports decode failures explicitly so invalid echoed bytes fail the
  /// state-machine assertion instead of disappearing.
  pub type Msg = String;

  /// Spin until the listener observes that the client connection is
  /// established.
  #[allow(
    clippy::single_call_fn,
    reason = "the client dialer names the connection wait used by the corrected send implementation"
  )]
  fn wait_until_connected(is_connected: &AtomicBool) {
    while !is_connected.load(ATOMIC_ORDER) {
      yield_now();
    }
  }

  /// Listener-side resources for the running echo server.
  pub struct ServerListener {
    /// Event listener that receives server network events.
    pub listener: NodeListener<()>,
    /// Node handler used to stop the server listener and send replies.
    pub handler:  NodeHandler<()>,
  }

  /// Dialer-side resources used by tests to address the running server.
  pub struct ServerDialer {
    /// Socket address chosen for the server listener.
    pub address: SocketAddr,
    /// Node handler used to stop the server.
    pub handler: NodeHandler<()>,
  }

  /// Listener-side resources for one connected client.
  pub struct ClientListener {
    /// Event listener that receives client network events.
    pub listener:     NodeListener<()>,
    /// Node handler used to stop the client listener.
    pub handler:      NodeHandler<()>,
    /// Server connection status, shared with the [`ClientDialer`].
    pub is_connected: Arc<AtomicBool>,
  }

  /// Dialer-side resources used by tests to send client messages.
  pub struct ClientDialer {
    /// Server endpoint this client sends messages to.
    pub server:       Endpoint,
    /// Node handler used to send messages and stop the client.
    pub handler:      NodeHandler<()>,
    /// Server connection status, shared with the [`ClientListener`].
    pub is_connected: Arc<AtomicBool>,
  }

  impl fmt::Debug for ServerListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      formatter.debug_struct("ServerListener").finish_non_exhaustive()
    }
  }

  impl fmt::Debug for ServerDialer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      formatter
        .debug_struct("ServerDialer")
        .field("address", &self.address)
        .finish_non_exhaustive()
    }
  }

  impl fmt::Debug for ClientListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      formatter.debug_struct("ClientListener").finish_non_exhaustive()
    }
  }

  impl fmt::Debug for ClientDialer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
      formatter
        .debug_struct("ClientDialer")
        .field("server", &self.server)
        .finish_non_exhaustive()
    }
  }

  impl EchoTransportExt for Transport {
    fn init_server<A>(self, addr: A) -> IoResult<(ServerDialer, ServerListener)>
    where
      A: ToSocketAddrs,
    {
      let (handler, listener) = node::split::<()>();

      let (_resource_id, address) = handler.network().listen(self, addr)?;

      Ok((
        ServerDialer {
          address,
          handler: handler.clone(),
        },
        ServerListener {
          listener,
          handler,
        },
      ))
    }

    fn init_client<A>(self, remote_addr: A) -> IoResult<(ClientListener, ClientDialer)>
    where
      A: ToRemoteAddr,
    {
      let (handler, listener) = node::split();
      let (server, _address) = handler.network().connect(self, remote_addr)?;

      let is_connected = Arc::new(AtomicBool::new(false));
      Ok((
        ClientListener {
          handler: handler.clone(),
          listener,
          is_connected: Arc::clone(&is_connected),
        },
        ClientDialer {
          server,
          handler,
          is_connected,
        },
      ))
    }
  }

  impl ServerListenerExt for ServerListener {
    fn run_server(self) {
      let Self {
        listener,
        handler,
      } = self;

      listener.for_each(move |event| match event.network() {
        NetEvent::Connected(..) => (), // Only generated at connect() calls.
        NetEvent::Accepted(_endpoint, _resource_id) => {
          // Only connection oriented protocols will generate this event
        }
        NetEvent::Message(endpoint, msg_bytes) => {
          let _status = handler.network().send(endpoint, msg_bytes);
        }
        NetEvent::Disconnected(_endpoint) => {
          // Only connection oriented protocols will generate this event
        }
      });
    }
  }

  impl ClientListenerExt for ClientListener {
    fn run_client<F>(self, mut on_msg: F)
    where
      F: FnMut(Result<Msg, FromUtf8Error>),
    {
      let Self {
        handler,
        listener,
        is_connected,
      } = self;

      listener.for_each(move |event| match event {
        NodeEvent::Network(net_event) => match net_event {
          NetEvent::Connected(_, established) => {
            is_connected.store(established, ATOMIC_ORDER);
          }
          NetEvent::Accepted(..) => {}
          NetEvent::Message(_, msg_bytes) => {
            on_msg(String::from_utf8(msg_bytes.to_vec()));
          }
          NetEvent::Disconnected(_) => {
            is_connected.store(false, ATOMIC_ORDER);
            handler.stop();
          }
        },
        NodeEvent::Signal(()) => {
          // unused
        }
      });
    }
  }

  impl ClientDialerExt for ClientDialer {
    fn msg_server(&mut self, msg: &str, implementation: SendImplementation) -> SendStatus {
      let output_data = msg.as_bytes();

      match implementation {
        SendImplementation::Immediate => {}
        SendImplementation::WaitUntilConnected => {
          wait_until_connected(&self.is_connected);
        }
      }

      self.handler.network().send(self.server, output_data)
    }
  }
}
