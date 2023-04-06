mod common;

use std::fmt::Debug;
use std::net::SocketAddr;

use robotica_backend::{
    devices::hdmi::{Command, Options},
    entities,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, ToSocketAddrs},
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

#[derive(Debug)]
enum ServerCommand {
    Shutdown,
}

#[derive(Debug)]
enum ServerStatus {
    Started,
}

async fn fake_server<A>(
    addr: A,
    instance: &str,
) -> Result<
    (
        mpsc::Sender<ServerCommand>,
        oneshot::Receiver<ServerStatus>,
        JoinHandle<()>,
        SocketAddr,
    ),
    std::io::Error,
>
where
    A: ToSocketAddrs + Clone + Send + Sync + Debug + 'static,
{
    let (tx, rx) = mpsc::channel(1);
    let (started_tx, started_rx) = oneshot::channel();
    let instance = instance.to_string();

    // Next up we create a TCP listener which will listen for incoming
    // connections. This TCP listener is bound to the address we determined
    // above and must be associated with an event loop.
    let listener = TcpListener::bind(&addr).await?;
    let addr = listener.local_addr().unwrap();
    println!("server({instance}): Listening on: {}", addr);
    started_tx.send(ServerStatus::Started).unwrap();

    let handle = tokio::spawn(async move {
        let mut handles = Vec::<JoinHandle<()>>::new();
        let mut rx = rx;
        loop {
            select! {
                socket = listener.accept() => {
                    let instance = instance.clone();
                    let handle = tokio::spawn(async move {
                        let (mut socket, _) = socket.unwrap();
                        println!("server({instance}): Got connection from: {}", socket.peer_addr().unwrap());

                        let mut buf = [0; 1024];
                        loop {
                            let n = socket.read(&mut buf).await.unwrap();
                            if n == 0 {
                                break;
                            }
                            println!("server({instance}): Received TCP Data");

                            // Write the data back
                            socket.write_all(&buf[0..n]).await.unwrap();
                        }

                        println!("server({instance}): Connection closed");
                    });
                    handles.push(handle);
                }
                Some(cmd) = rx.recv() => {
                    println!("server({instance}): Received command: {cmd:?}");
                    match cmd {
                        ServerCommand::Shutdown => { break; },
                    }
                }
                else => {
                    println!("server({instance}): Select failed");
                    break;
                }
            }
        }

        println!("server({instance}): Shutting down");
        for handle in handles {
            // Kill the connection.
            handle.abort();

            // Ignore the JoinError::Cancelled error.
            let _ = handle.await;
        }

        println!("server({instance}): finished");
    });

    Ok((tx, started_rx, handle, addr))
}

#[tokio::test]
async fn test_client_once() {
    common::setup();

    let options = Options {
        disable_polling: true,
    };
    let addr = "127.0.0.2:0";

    println!("test: starting server");
    let (server, started, server_handle, addr) = fake_server(addr, "only").await.unwrap();
    let _ = started.await;

    println!("test: starting client");
    let (client, rx) = entities::create_stateless_entity("test");
    let (rx, client_handle) = robotica_backend::devices::hdmi::run(addr, rx, &options);
    let mut rx_s = rx.subscribe().await;

    println!("test: sending test command");
    client.try_send(Command::SetInput(2, 1));

    println!("test: waiting for client to finish");
    let state = rx_s.recv().await.unwrap();
    assert_eq!(state, Ok([Some(2), None, None, None]));

    println!("test: Shutting down client");
    client.try_send(Command::Shutdown);
    client_handle.await.unwrap();

    println!("test: Shutting down server");
    server.send(ServerCommand::Shutdown).await.unwrap();
    server_handle.await.unwrap();

    println!("test: done");
}

#[tokio::test]
async fn test_client_reconnect() {
    common::setup();

    let addr = "127.0.0.1:0";
    let options = Options {
        disable_polling: true,
    };

    println!("test: starting server");
    let (server, started, server_handle, addr) = fake_server(addr, "first").await.unwrap();
    let _ = started.await;

    println!("test: starting client");
    let (client, rx) = entities::create_stateless_entity("test");
    let (rx, client_handle) = robotica_backend::devices::hdmi::run(addr, rx, &options);
    let mut rx_s = rx.subscribe().await;

    println!("test: sending test command");
    client.try_send(Command::SetInput(2, 1));

    println!("test: waiting for client to finish");
    let state = rx_s.recv().await.unwrap();
    assert_eq!(state, Ok([Some(2), None, None, None]));

    println!("test: Restarting server");
    server.send(ServerCommand::Shutdown).await.unwrap();
    server_handle.await.unwrap();
    let (server, started, server_handle, _addr) = fake_server(addr, "second").await.unwrap();
    let _ = started.await;

    println!("test: sending test command after server restart");
    client.try_send(Command::SetInput(3, 2));

    println!("test: waiting for client to finish");
    let state = rx_s.recv().await.unwrap();
    assert_eq!(state, Ok([Some(2), Some(3), None, None]));

    println!("test: Shutting down client");
    client.try_send(Command::Shutdown);
    client_handle.await.unwrap();

    println!("test: Shutting down server");
    server.send(ServerCommand::Shutdown).await.unwrap();
    server_handle.await.unwrap();

    println!("test: done");
}
