#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::Interest;
use tokio::net::{TcpListener, TcpStream};
use tokio_test::task;
use tokio_test::{assert_pending, assert_ready_ok};

use std::io;

#[tokio::test]
async fn try_read_write() {
    const DATA: &[u8] = b"this is some data to write to the socket";

    // Create listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

    // Create socket pair
    let client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (server, _) = listener.accept().await.unwrap();
    let mut written = DATA.to_vec();

    // Track the server receiving data
    let mut readable = task::spawn(server.readable());
    assert_pending!(readable.poll());

    // Write data.
    client.writable().await.unwrap();
    assert_eq!(DATA.len(), client.try_write(DATA).unwrap());

    // The task should be notified
    while !readable.is_woken() {
        tokio::task::yield_now().await;
    }

    // Fill the write buffer
    loop {
        // Still ready
        let mut writable = task::spawn(client.writable());
        assert_ready_ok!(writable.poll());

        match client.try_write(DATA) {
            Ok(n) => written.extend(&DATA[..n]),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => panic!("error = {:?}", e),
        }
    }

    {
        // Write buffer full
        let mut writable = task::spawn(client.writable());
        assert_pending!(writable.poll());

        // Drain the socket from the server end
        let mut read = vec![0; written.len()];
        let mut i = 0;

        while i < read.len() {
            server.readable().await.unwrap();

            let n = server.try_read(&mut read[i..]).unwrap();
            i += n;
        }

        assert_eq!(read, written);
    }

    // Now, we listen for shutdown
    drop(client);

    loop {
        let ready = server.ready(Interest::READABLE).await.unwrap();

        if ready.is_read_closed() {
            return;
        } else {
            tokio::task::yield_now().await;
        }
    }
}
