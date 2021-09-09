use std::{collections::VecDeque, future::Future, ops::{Deref, DerefMut}, sync::{Arc, Mutex, Weak}};

use futures_util::future;
use tokio::sync::{oneshot};
use tower::{Service, ServiceExt};

use super::started;
use crate::pool::started::Started;

pub(crate) enum Never {}

/// Connection Pool for reuse connections
pub struct Pool<T> {
    // share between threads
    inner: Arc<Mutex<Inner<T>>>,
}

impl<T> Clone for Pool<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

pub struct Pooled<T> {
    pub inner: Option<T>,
    pool: Weak<Mutex<Inner<T>>>,
}

impl<T> AsRef<T> for Pooled<T> {
    fn as_ref(&self) -> &T {
        self.inner.as_ref().expect("not dropped")
    }
}

impl<T> AsMut<T> for Pooled<T> {
    fn as_mut(&mut self) -> &mut T {
        self.inner.as_mut().expect("not dropped")
    }
}

impl<T> Deref for Pooled<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.as_ref()
    }
}

impl<T> DerefMut for Pooled<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.as_mut()
    }
}

impl<T> Pooled<T> {
    pub fn new(t: T, pool: Weak<Mutex<Inner<T>>>) -> Self {
        Pooled {
            inner: Some(t),
            pool,
        }
    }
}

impl<T> Drop for Pooled<T> {
    fn drop(&mut self) {
        let t = self.inner.take().unwrap();
        if let Some(pool) = self.pool.upgrade() {
            if let Ok(mut pool) = pool.lock() {
                pool.put(t);
            }
        }
    }
}

pub trait ElemBuilder {
    type Item: Elem;
    type Future: Future<Output = Self::Item>;
    fn build(&self) -> Self::Future;
}

pub trait Elem {}

pub struct Inner<T> {
    idle: Vec<T>,
    waiters: VecDeque<oneshot::Sender<T>>,
    max_idle: usize,
}

impl<T> Inner<T> {
    pub fn put(&mut self, t: T) {
        let mut value = Some(t);
        if !self.waiters.is_empty() {
            while let Some(waiter) = self.waiters.pop_front() {
                if !waiter.is_closed() {
                    let t = value.take().unwrap();
                    match waiter.send(t) {
                        Ok(()) => {
                            break;
                        }
                        Err(t) => {
                            value = Some(t);
                        }
                    }
                }
            }
        }

        if let Some(t) = value {
            if self.idle.len() < self.max_idle {
                self.idle.push(t);
            }
        }
    }
}

impl<T> Pool<T>
where
    T: Send + 'static,
{
    pub fn new(max_idle: usize) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            idle: Vec::new(),
            waiters: VecDeque::new(),
            max_idle,
        }));
        Self { inner }
    }

    pub async fn get<MT>(&self, mt: MT) -> anyhow::Result<Pooled<T>>
    where
        MT: Service<(), Response = T> + Send + 'static,
        MT::Error: Into<anyhow::Error>,
        MT::Future: Unpin + Send + 'static,
    {
        let rx = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(idle) = inner.idle.pop() {
                return Ok(Pooled {
                    inner: Some(idle),
                    pool: Arc::downgrade(&self.inner),
                });
            }

            let (tx, rx) = oneshot::channel();
            inner.waiters.push_back(tx);
            // try create
            drop(inner);
            rx
        };

        let lazy_fut = { || mt.oneshot(()) };
        match future::select(rx, started::lazy(lazy_fut)).await {
            future::Either::Left((Ok(v), fut)) => {
                if fut.started() {
                    let inner = Arc::downgrade(&self.inner);
                    tokio::spawn(async move {
                        if let Ok(t) = fut.await {
                            let _ = Pooled::new(t, inner);
                        }
                    });
                }
                Ok(Pooled::new(v, Arc::downgrade(&self.inner)))
            }
            future::Either::Right((Ok(v), _)) => Ok(Pooled::new(v, Arc::downgrade(&self.inner))),
            future::Either::Left((Err(e), _)) => Err(e.into()),
            future::Either::Right((Err(e), _)) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pin_project::pin_project;
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;
    use tokio::io::AsyncRead;
    use tokio::io::AsyncWrite;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixStream,
    };
    use tower::Service;

    #[pin_project]
    struct MockConnection(#[pin] UnixStream);

    impl AsyncRead for MockConnection {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            self.project().0.poll_read(cx, buf)
        }
    }

    impl AsyncWrite for MockConnection {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            self.project().0.poll_write(cx, buf)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            self.project().0.poll_flush(cx)
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            self.project().0.poll_shutdown(cx)
        }
    }

    struct MockMakeTransport;

    impl<Key> Service<Key> for MockMakeTransport {
        type Response = MockConnection;

        type Error = io::Error;

        type Future = Pin<Box<dyn Future<Output = Result<MockConnection, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: Key) -> Self::Future {
            Box::pin(async {
                let (tx, rx) = UnixStream::pair()?;
                tokio::spawn(async move {
                    let mut line = String::new();
                    let mut rx = BufReader::new(rx);
                    loop {
                        rx.read_line(&mut line).await.unwrap();
                        println!("{}iiii", line);
                    }
                });
                Ok(MockConnection(tx))
            })
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_pool() {
        let pool = Pool::new(10);
        let mc = MockMakeTransport;
        let mut tx = pool.get(mc).await.unwrap();
        (&mut *tx).write(b"hello world\n").await.unwrap();
        drop(pool);
        drop(tx);
        tokio::time::sleep(tokio::time::Duration::from_secs(50000)).await;
        println!("end");
    }
}
