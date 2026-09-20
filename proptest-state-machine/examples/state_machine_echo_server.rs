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
//! Pass `--correct` to wait for connection readiness before sending.

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fmt;
use std::io::Error as IoError;
use std::io::Result as IoResult;
use std::mem::take;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::string::FromUtf8Error;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;
use std::vec::IntoIter;

use message_io::network::SendStatus;
use message_io::network::ToRemoteAddr;
use message_io::network::Transport;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::Config;
use proptest_state_machine::ReferenceStateMachine;
use proptest_state_machine::StateMachinePropertyResult;
use proptest_state_machine::StateMachineTest;
use proptest_state_machine::prop_state_machine;
use proptest_state_machine::strict_state_machine_config;
use strict_test_support::ComparisonFailure;
use strict_test_support::OptionFailure;
use strict_test_support::PredicateFailure;
use strict_test_support::ResultFailure;
use strict_test_support::ensure_eq;
use strict_test_support::ensure_ok;
use strict_test_support::ensure_some;
use strict_test_support::ensure_that;
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
}

// Setup the state machine test using the `prop_state_machine!` macro
prop_state_machine! {
    #![proptest_config(Config {
        // Enable verbose mode to make the state machine test print the
        // transitions for each case.
        verbose: 1,
        // This is the inner driver config. Set PROPTEST_CASES=10 when running
        // the example to cap the outer runner's thread/socket workload.
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

fn main() -> StateMachinePropertyResult<EchoServerTest> {
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
#[derive(Debug, Default)]
struct EchoServerTest {
  /// The running server, if the model says it has been started.
  server:  Option<TestServer>,
  /// Running clients indexed by their model client IDs.
  clients: HashMap<ClientId, TestClient>,
}

/// Running server resources held by the concrete state machine.
#[derive(Debug)]
struct TestServer {
  /// A server dialer can be used to send message to clients and to shut-down
  /// the server.
  dialer:          ServerDialer,
  /// The a handle of a thread that runs the server listener.
  listener_handle: thread::JoinHandle<()>,
}

/// Running client resources held by the concrete state machine.
#[derive(Debug)]
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

/// Native server resources and the completed listener join.
struct StoppedServer {
  /// The handler and bound address remain inspectable after shutdown.
  dialer: ServerDialer,
  /// Preserve a listener's native panic payload if joining failed.
  joined: thread::Result<()>,
}

/// Native client resources and the completed listener join.
struct StoppedClient {
  /// The model identity of the stopped listener.
  id:        ClientId,
  /// The handler and endpoint remain inspectable after shutdown.
  dialer:    ClientDialer,
  /// Preserve unread messages and their native decode results.
  msgs_recv: Receiver<Result<Msg, FromUtf8Error>>,
  /// Preserve a listener's native panic payload if joining failed.
  joined:    thread::Result<()>,
}

/// Evidence returned by the concrete transition that produced it.
enum EchoObservation {
  /// The address bound for the running server.
  ServerStarted(SocketAddr),
  /// Completed server shutdown followed by completed client shutdowns.
  ServerStopped(StoppedServer, Vec<StoppedClient>),
  /// Client identity, connected server address, and prior map entry.
  ClientStarted(ClientId, SocketAddr, Option<TestClient>),
  /// Completed shutdown of one client.
  ClientStopped(StoppedClient),
  /// The native send result and both compared messages.
  Message(ClientId, SendStatus, (Msg, Msg)),
}

impl fmt::Debug for StoppedServer {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("StoppedServer")
      .field("dialer", &self.dialer)
      .field("joined", &self.joined)
      .finish()
  }
}

impl fmt::Debug for StoppedClient {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("StoppedClient")
      .field("id", &self.id)
      .field("dialer", &self.dialer)
      .field("msgs_recv", &self.msgs_recv)
      .field("joined", &self.joined)
      .finish()
  }
}

impl fmt::Debug for EchoObservation {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match *self {
      Self::ServerStarted(address) => formatter.debug_tuple("ServerStarted").field(&address).finish(),
      Self::ServerStopped(ref server, ref clients) => formatter.debug_tuple("ServerStopped").field(server).field(clients).finish(),
      Self::ClientStarted(id, address, ref previous) => formatter
        .debug_tuple("ClientStarted")
        .field(&id)
        .field(&address)
        .field(previous)
        .finish(),
      Self::ClientStopped(ref client) => formatter.debug_tuple("ClientStopped").field(client).finish(),
      Self::Message(id, status, ref messages) => formatter
        .debug_tuple("Message")
        .field(&id)
        .field(&status)
        .field(messages)
        .finish(),
    }
  }
}

/// Operation failures retain observations and resources consumed before stopping.
#[derive(Debug, thiserror::Error)]
enum EchoOperationFailure {
  /// A socket operation failed before creating the listener thread.
  #[error(transparent)]
  Socket(#[from] ResultFailure<IoError>),
  /// The server bound its socket, but its listener thread could not start.
  #[error("could not start the listener for {dialer:?}: {source}")]
  ServerSpawn {
    /// The bound server remains identifiable after the spawn failure.
    dialer: ServerDialer,
    /// The native operating-system thread creation failure.
    source: ResultFailure<IoError>,
  },
  /// The client connected its socket, but its listener thread could not start.
  #[error("could not start client {id} with {dialer:?} and receiver {msgs_recv:?}: {source}")]
  ClientSpawn {
    /// The identity requested by the transition.
    id:        ClientId,
    /// The connected endpoint and handler produced before spawning.
    dialer:    ClientDialer,
    /// The receive channel created before spawning the listener.
    msgs_recv: Receiver<Result<Msg, FromUtf8Error>>,
    /// The native operating-system thread creation failure.
    source:    ResultFailure<IoError>,
  },
  /// Server shutdown was requested without a running server.
  #[error(transparent)]
  MissingServer(#[from] OptionFailure<TestServer>),
  /// A client could not find a running server address.
  #[error(transparent)]
  MissingAddress(#[from] OptionFailure<SocketAddr>),
  /// A requested client identity is absent; the enclosing failure owns the map.
  #[error("client {id} is not running for message {message:?}")]
  MissingClient {
    /// The absent identity requested by the transition.
    id:      ClientId,
    /// The unattempted message, when this was a send operation.
    message: Option<Msg>,
  },
  /// Insertion unexpectedly displaced an existing client.
  #[error("client {id} replaced an existing entry: {source}")]
  ReplacedClient {
    /// The identity whose insertion displaced a running listener.
    id:     ClientId,
    /// The displaced client and the failed absence check.
    source: Box<PredicateFailure<Option<TestClient>>>,
  },
  /// The server listener did not join cleanly.
  #[error(transparent)]
  ServerJoin(#[from] PredicateFailure<StoppedServer>),
  /// A single client listener did not join cleanly.
  #[error(transparent)]
  ClientJoin(#[from] PredicateFailure<StoppedClient>),
  /// Bulk shutdown stopped at this listener, retaining the unattempted clients.
  #[error("client shutdown failed after {stopped:?}, with {remaining:?} remaining: {source}")]
  ClientShutdown {
    /// Completed joins preceding the failure.
    stopped:   Vec<StoppedClient>,
    /// The failing listener's resources and native join outcome.
    source:    Box<PredicateFailure<StoppedClient>>,
    /// Clients whose shutdown has not been attempted.
    remaining: IntoIter<(ClientId, TestClient)>,
  },
  /// The server has already joined when a subsequent client shutdown fails.
  #[error("client shutdown failed after stopping {server:?}: {source}")]
  ServerClients {
    /// The completed server join preceding client shutdown.
    server: StoppedServer,
    /// The original client shutdown failure and its progress.
    source: Box<Self>,
  },
  /// Preserve the message and native send status before a receive is attempted.
  #[error("client {id} could not send {message:?}: {source}")]
  Send {
    /// The sending client's model identity.
    id:      ClientId,
    /// The original message supplied to the network controller.
    message: Msg,
    /// The rejected native send status.
    source:  PredicateFailure<SendStatus>,
  },
  /// The native channel failure follows a successful send.
  #[error("client {id} sent {message:?} with {status:?}, but no response arrived: {source}")]
  Receive {
    /// The client awaiting its echo.
    id:      ClientId,
    /// The message already submitted to the network controller.
    message: Msg,
    /// The successful native send observation.
    status:  SendStatus,
    /// The original receive timeout or disconnection.
    source:  ResultFailure<mpsc::RecvTimeoutError>,
  },
  /// The native UTF-8 failure retains every received byte.
  #[error("client {id} sent {message:?} with {status:?}, but decoding failed: {source}")]
  Decode {
    /// The client whose echo was received.
    id:      ClientId,
    /// The original sent message.
    message: Msg,
    /// The native send observation before receiving bytes.
    status:  SendStatus,
    /// The invalid bytes and their UTF-8 decoding failure.
    source:  ResultFailure<FromUtf8Error>,
  },
  /// Both different message values survive the comparison.
  #[error("client {id} sent with {status:?}, but received a different echo: {source}")]
  Echo {
    /// The client whose decoded echo disagreed.
    id:     ClientId,
    /// The successful native send observation.
    status: SendStatus,
    /// Both complete compared messages and their comparison failure.
    source: ComparisonFailure<Msg, Msg>,
  },
}

/// The reached concrete state survives a failed consuming application hook.
#[derive(Debug, thiserror::Error)]
#[error("echo transition failed with state {state:?}: {source}")]
struct EchoFailure {
  /// Live resources still in the state machine's custody.
  state:  EchoServerTest,
  /// The original operation failure with completed and unattempted work.
  source: EchoOperationFailure,
}

/// The send observation and both complete compared messages.
type EchoExchange = (SendStatus, (Msg, Msg));

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
  fn start_server(&mut self, ref_state: &RefState) -> Result<SocketAddr, EchoOperationFailure> {
    let (dialer, listener) = ensure_ok(
      ref_state.transport.init_server("127.0.0.1:0"),
      "the server socket binds and listens",
    )?;
    let address = dialer.address;
    let listener_handle = match ensure_ok(
      thread::Builder::new().spawn(move || listener.run_server()),
      "the server listener thread starts",
    ) {
      Ok(handle) => handle,
      Err(source) => {
        return Err(EchoOperationFailure::ServerSpawn {
          dialer,
          source,
        });
      }
    };
    self.server = Some(TestServer {
      dialer,
      listener_handle,
    });
    Ok(address)
  }

  /// Stop every concrete client in the original identifier order.
  fn stop_all_clients(&mut self) -> Result<Vec<StoppedClient>, EchoOperationFailure> {
    let mut clients = take(&mut self.clients).into_iter().collect::<Vec<_>>();
    clients.sort_by_key(|entry| entry.0);
    let mut remaining = clients.into_iter();
    let mut stopped = Vec::new();
    while let Some((id, client)) = remaining.next() {
      client.dialer.handler.stop();
      let observed = StoppedClient {
        id,
        dialer: client.dialer,
        msgs_recv: client.msgs_recv,
        joined: client.listener_handle.join(),
      };
      match ensure_that(observed, "a client listener thread stops cleanly", |joined| joined.joined.is_ok()) {
        Ok(completed) => stopped.push(completed),
        Err(source) => {
          return Err(EchoOperationFailure::ClientShutdown {
            stopped,
            source: Box::new(source),
            remaining,
          });
        }
      }
    }
    Ok(stopped)
  }

  /// Join the server before attempting to stop any disconnected clients.
  fn stop_server(&mut self) -> Result<(StoppedServer, Vec<StoppedClient>), EchoOperationFailure> {
    let server = ensure_some(self.server.take(), "stopping the server requires a running server")?;
    server.dialer.handler.stop();
    let stopped_server = ensure_that(
      StoppedServer {
        dialer: server.dialer,
        joined: server.listener_handle.join(),
      },
      "the server listener thread stops cleanly",
      |observed| observed.joined.is_ok(),
    )?;
    let clients = if self.clients.is_empty() {
      Ok(Vec::new())
    } else {
      self.stop_all_clients()
    };
    match clients {
      Ok(stopped_clients) => Ok((stopped_server, stopped_clients)),
      Err(source) => Err(EchoOperationFailure::ServerClients {
        server: stopped_server,
        source: Box::new(source),
      }),
    }
  }

  /// Start a concrete client connected to the current concrete server.
  fn start_client(&mut self, ref_state: &RefState, id: ClientId) -> Result<(SocketAddr, Option<TestClient>), EchoOperationFailure> {
    let server_addr = ensure_some(
      self.server.as_ref().map(|server| server.dialer.address),
      "starting a client requires a running server",
    )?;
    let (listener, dialer) = ensure_ok(
      ref_state.transport.init_client(server_addr),
      "the client connects to the server address",
    )?;
    let (msgs_send, msgs_recv) = mpsc::channel();
    let listener_handle = match ensure_ok(
      thread::Builder::new().spawn(move || {
        listener.run_client(|msg| {
          drop(msgs_send.send(msg));
        });
      }),
      "the client listener thread starts",
    ) {
      Ok(handle) => handle,
      Err(source) => {
        return Err(EchoOperationFailure::ClientSpawn {
          id,
          dialer,
          msgs_recv,
          source,
        });
      }
    };
    let previous = ensure_that(
      self.clients.insert(id, TestClient {
        dialer,
        listener_handle,
        msgs_recv,
      }),
      "starting a client creates a new concrete client",
      Option::is_none,
    )
    .map_err(|source| EchoOperationFailure::ReplacedClient {
      id,
      source: Box::new(source),
    })?;
    Ok((server_addr, previous))
  }

  /// Stop one concrete client by model identifier.
  fn stop_client(&mut self, id: ClientId) -> Result<StoppedClient, EchoOperationFailure> {
    let client = self.clients.remove(&id).ok_or(EchoOperationFailure::MissingClient {
      id,
      message: None,
    })?;
    client.dialer.handler.stop();
    ensure_that(
      StoppedClient {
        id,
        dialer: client.dialer,
        msgs_recv: client.msgs_recv,
        joined: client.listener_handle.join(),
      },
      "the stopped client listener thread stops cleanly",
      |observed| observed.joined.is_ok(),
    )
    .map_err(EchoOperationFailure::ClientJoin)
  }

  /// Send a message and preserve each native observation through the echo check.
  fn send_client_message(&mut self, id: ClientId, msg: Msg) -> Result<EchoExchange, EchoOperationFailure> {
    let Some(client) = self.clients.get_mut(&id) else {
      return Err(EchoOperationFailure::MissingClient {
        id,
        message: Some(msg),
      });
    };
    let implementation = if env::args_os().any(|argument| argument == "--correct") {
      SendImplementation::CORRECT
    } else {
      SendImplementation::BUGGY_DEFAULT
    };
    let status = match ensure_that(
      client.dialer.msg_server(&msg, implementation),
      "client send reaches the network controller",
      |status| *status == SendStatus::Sent,
    ) {
      Ok(status) => status,
      Err(source) => {
        return Err(EchoOperationFailure::Send {
          id,
          message: msg,
          source,
        });
      }
    };

    // Pass `--correct` to wait until connection readiness.
    // The intentionally wrong path reports a non-Sent status or a timeout.
    let decoded = match ensure_ok(
      client.msgs_recv.recv_timeout(Duration::from_secs(1)),
      "the server responds to the client",
    ) {
      Ok(received) => received,
      Err(source) => {
        return Err(EchoOperationFailure::Receive {
          id,
          message: msg,
          status,
          source,
        });
      }
    };
    let received = match ensure_ok(decoded, "the server response is valid UTF-8") {
      Ok(message) => message,
      Err(source) => {
        return Err(EchoOperationFailure::Decode {
          id,
          message: msg,
          status,
          source,
        });
      }
    };
    match ensure_eq(received, msg, "the server echoes the client's message unchanged") {
      Ok(messages) => Ok((status, messages)),
      Err(source) => Err(EchoOperationFailure::Echo {
        id,
        status,
        source,
      }),
    }
  }
}

impl StateMachineTest for EchoServerTest {
  type SystemUnderTest = Self;
  type Reference = RefState;
  type Failure = Box<EchoFailure>;
  type TransitionEvidence = EchoObservation;
  type InvariantEvidence = ();

  fn init_test(_ref_state: &RefState) -> Self::SystemUnderTest {
    Self::default()
  }

  fn apply(mut state: Self, ref_state: &RefState, transition: Transition) -> Result<(Self, EchoObservation), Self::Failure> {
    let result = match transition {
      Transition::StartServer => state.start_server(ref_state).map(EchoObservation::ServerStarted),
      Transition::StopServer => state
        .stop_server()
        .map(|(server, clients)| EchoObservation::ServerStopped(server, clients)),
      Transition::StartClient(id) => state
        .start_client(ref_state, id)
        .map(|(address, previous)| EchoObservation::ClientStarted(id, address, previous)),
      Transition::StopClient(id) => state.stop_client(id).map(EchoObservation::ClientStopped),
      Transition::ClientMsg(id, msg) => state
        .send_client_message(id, msg)
        .map(|(status, messages)| EchoObservation::Message(id, status, messages)),
    };
    match result {
      Ok(observed) => Ok((state, observed)),
      Err(source) => Err(Box::new(EchoFailure {
        state,
        source,
      })),
    }
  }

  fn check_invariants(_state: &Self, _ref_state: &RefState) -> Result<(), Self::Failure> {
    // This example checks each transition's post-condition in apply.
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Complete native result of sending through the corrected readiness path.
  type ReadyExchange = Option<(SendStatus, Result<Result<Msg, FromUtf8Error>, mpsc::RecvTimeoutError>)>;
  /// Live state, corrected send, and every completed lifecycle operation.
  type Lifecycle = (EchoServerTest, ReadyExchange, Vec<Result<EchoObservation, EchoOperationFailure>>);
  /// Failed consuming application retains its entire concrete state.
  type FailedApplication = Result<(EchoServerTest, EchoObservation), Box<EchoFailure>>;

  #[test]
  fn real_socket_lifecycle_preserves_messages_and_joined_resources() -> Result<(), Box<PredicateFailure<Lifecycle>>> {
    let reference = RefState {
      is_server_up: true,
      clients:      HashSet::from([2, 9]),
      transport:    Transport::FramedTcp,
    };
    let mut state = EchoServerTest::default();
    let mut operations = vec![state.start_server(&reference).map(EchoObservation::ServerStarted)];
    for id in [9, 2] {
      operations.push(
        state
          .start_client(&reference, id)
          .map(|(address, previous)| EchoObservation::ClientStarted(id, address, previous)),
      );
    }
    let ready = state.clients.get_mut(&9).map(|client| {
      let sent = client.dialer.msg_server("ready", SendImplementation::CORRECT);
      let received = client.msgs_recv.recv_timeout(Duration::from_secs(1));
      (sent, received)
    });
    operations.push(
      state
        .send_client_message(9, "echo".to_owned())
        .map(|(status, messages)| EchoObservation::Message(9, status, messages)),
    );
    operations.push(state.stop_client(9).map(EchoObservation::ClientStopped));
    operations.push(
      state
        .stop_server()
        .map(|(server, clients)| EchoObservation::ServerStopped(server, clients)),
    );
    ensure_that(
      (state, ready, operations),
      "real listeners echo both sends, join in lifecycle order, and retain native shutdown resources",
      |observed| {
        let &[
          Ok(EchoObservation::ServerStarted(address)),
          Ok(EchoObservation::ClientStarted(9, first, None)),
          Ok(EchoObservation::ClientStarted(2, second, None)),
          Ok(EchoObservation::Message(9, SendStatus::Sent, ref messages)),
          Ok(EchoObservation::ClientStopped(ref stopped)),
          Ok(EchoObservation::ServerStopped(ref server, ref clients)),
        ] = observed.2.as_slice()
        else {
          return false;
        };
        observed.0.server.is_none()
          && observed.0.clients.is_empty()
          && matches!(observed.1, Some((SendStatus::Sent, Ok(Ok(ref message)))) if message == "ready")
          && address == first
          && address == second
          && address == server.dialer.address
          && messages == &("echo".to_owned(), "echo".to_owned())
          && stopped.id == 9
          && stopped.joined.is_ok()
          && server.joined.is_ok()
          && clients.len() == 1
          && clients.first().is_some_and(|client| client.id == 2 && client.joined.is_ok())
      },
    )
    .map(drop)
    .map_err(Box::new)
  }

  #[test]
  fn unavailable_resources_return_typed_failures_with_state_and_message() -> Result<(), Box<PredicateFailure<[FailedApplication; 3]>>> {
    let reference = RefState {
      is_server_up: false,
      clients:      HashSet::new(),
      transport:    Transport::Tcp,
    };
    let failed_applications = [
      EchoServerTest::apply(EchoServerTest::default(), &reference, Transition::StartClient(7)),
      EchoServerTest::apply(EchoServerTest::default(), &reference, Transition::StopServer),
      EchoServerTest::apply(EchoServerTest::default(), &reference, Transition::ClientMsg(7, "unsent".to_owned())),
    ];
    ensure_that(
      failed_applications,
      "missing resources fail before effects and preserve the unattempted message",
      |observed| {
        let &[Err(ref address), Err(ref server), Err(ref client)] = observed else {
          return false;
        };
        observed.iter().all(|result| {
          result
            .as_ref()
            .is_err_and(|failure| failure.state.server.is_none() && failure.state.clients.is_empty())
        }) && matches!(address.source, EchoOperationFailure::MissingAddress(_))
          && matches!(server.source, EchoOperationFailure::MissingServer(_))
          && matches!(client.source, EchoOperationFailure::MissingClient { id: 7, message: Some(ref message) } if message == "unsent")
      },
    )
    .map(drop)
    .map_err(Box::new)
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
