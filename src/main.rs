// TODO: create trait of in memory cache
// TODO: use LRU impl for now
// TODO: configure how many LRU we want
// TODO: xxhash on keys to choose the LRU
// TODO: metrics

mod cache;
mod sieve;
use bytes::Bytes;
use cache::CacheWithTTLEntry;
use std::{cell::RefCell, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpSocket, TcpStream},
    select,
};

use std::io;

use crate::{cache::CacheWithTTL, sieve::ESieve};

type SharedCache = Arc<RefCell<CacheWithTTL<Bytes, ESieve<CacheWithTTLEntry<Bytes>>>>>;

async fn service(mut socket: TcpStream, addr: SocketAddr, cache: SharedCache) {
    println!("new client: {}", addr);
    let mut buf = [0; 1024];

    loop {
        let readwrite = async {
            // TODO: actual impl here
            let n = match socket.read(&mut buf).await {
                // socket closed
                Ok(0) => return,
                Ok(n) => n,
                Err(e) => {
                    eprintln!("failed to read from socket; err = {:?}", e);
                    return;
                }
            };

            // Write the data back
            // FIXME: this is not cancel safe, should move the select! on the reading part only
            eprintln!(
                "Receieved this buffer from {} => {:?} - but returning a value from cache",
                addr,
                &buf[0..n]
            );
            if let Some(bytes) = cache.borrow_mut().get("user-id-1") {
                if let Err(e) = socket.write_all(&bytes).await {
                    eprintln!("failed to write to socket; err = {:?}", e);
                    return;
                }
            } else {
                eprintln!("no cache value found!")
            }
        };

        select! {
            _ = tokio::signal::ctrl_c() => {
                println!("closing {} socket", addr);
                if let Err(e) = socket.shutdown().await {
                    eprintln!("failed to shutdown socket {}; err = {:?}", addr, e);
                }
                return;
            }
            _ = readwrite => {}
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // TODO: 1 shared cache, should be multiple
    // TODO: capacity should be total_capacity / cache_count
    let mut cache = CacheWithTTL::<Bytes, ESieve<_>>::new(1000);
    let cache = Arc::new(RefCell::new(cache));
    // cache.set(
    //     "key-1",
    //     Bytes::from_static(b"value-1"),
    //     Duration::from_secs(60 * 60 * 3),
    // );
    // assert_eq!(cache.get("key-1"), Some(Bytes::from_static(b"value-1")));

    // TODO: makes PORT configurable
    let addr = "127.0.0.1:8080".parse().unwrap();

    let socket = TcpSocket::new_v4()?;
    // On platforms with Berkeley-derived sockets, this allows to quickly
    // rebind a socket, without needing to wait for the OS to clean up the
    // previous one.
    //
    // On Windows, this allows rebinding sockets which are actively in use,
    // which allows “socket hijacking”, so we explicitly don't set it here.
    // https://docs.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse
    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;

    let listener = socket.listen(1024)?; // TODO: makes backlog configurable
    let mut tasks = Vec::new();

    loop {
        let listen = async {
            let (socket, addr) = listener.accept().await.unwrap();
            tasks.push(tokio::spawn(async move {
                service(socket, addr, cache.clone()).await
            }));
        };

        select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            _ = listen => {}
        }
    }

    println!("closing server...");
    for t in tasks {
        t.await?;
    }

    Ok(())
}
