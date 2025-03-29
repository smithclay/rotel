// SPDX-License-Identifier: Apache-2.0

use flume::r#async::{RecvStream, SendFut};
use flume::{Receiver, Sender};
use std::fmt;

pub struct BoundedSender<T> {
    tx: Sender<T>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    Disconnected,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendError::Disconnected => write!(f, "channel disconnected"),
        }
    }
}

impl<T> BoundedSender<T> {
    pub async fn send(&self, item: T) -> Result<(), SendError> {
        match self.tx.send_async(item).await {
            Ok(()) => Ok(()),
            Err(_e) => Err(SendError::Disconnected), // receiver closed
        }
    }

    pub fn send_async(&self, item: T) -> SendFut<'_, T> {
        self.tx.send_async(item)
    }
}

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        // this could be derived, but placeholder for now
        Self {
            tx: self.tx.clone(),
        }
    }
}

#[derive(Clone)]
pub struct BoundedReceiver<T> {
    rx: Receiver<T>,
}

impl<T> BoundedReceiver<T> {
    pub async fn next(&mut self) -> Option<T> {
        match self.rx.recv_async().await {
            Ok(item) => Some(item),
            Err(_e) => None, // disconnected
        }
    }

    pub fn stream(&self) -> RecvStream<'_, T> {
        self.rx.stream()
    }
}

pub fn bounded<T>(size: usize) -> (BoundedSender<T>, BoundedReceiver<T>) {
    let (tx, rx) = flume::bounded::<T>(size);

    let sender = BoundedSender { tx };
    let receiver = BoundedReceiver { rx };

    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use super::{bounded, SendError};
    use tokio_test::{assert_ok, assert_pending, assert_ready, task::spawn};

    #[tokio::test]
    async fn basics() {
        let (tx, mut rx) = bounded(3);

        let msg = 10;

        // wrap futures
        let mut send1 = spawn(async { tx.send(msg).await });
        let mut recv1 = spawn(async { rx.next().await });

        // both asleep
        assert!(!send1.is_woken());
        assert!(!recv1.is_woken());

        // receiver should be in pending state
        assert_pending!(recv1.poll());

        assert_ok!(assert_ready!(send1.poll()));

        assert!(recv1.is_woken());

        assert_eq!(Some(msg), assert_ready!(recv1.poll()));

        drop(send1);
        drop(recv1);

        let mut recv2 = spawn(async { rx.next().await });

        drop(tx);
        // receives None since send channel was closed
        assert_eq!(None, assert_ready!(recv2.poll()));
    }

    #[tokio::test]
    async fn sender_blocks_on_full() {
        let (tx, mut rx) = bounded(1);

        let msg = 10;

        // wrap futures
        let mut send1 = spawn(async { tx.send(msg).await });
        let mut recv1 = spawn(async { rx.next().await });

        // receiver should be in pending state
        assert!(!recv1.is_woken());

        assert_ok!(assert_ready!(send1.poll()));

        drop(send1);
        let mut send2 = spawn(async { tx.send(msg).await });

        // Now blocks
        assert_pending!(send2.poll());

        assert_eq!(Some(msg), assert_ready!(recv1.poll()));

        // now this is ok
        assert_ok!(assert_ready!(send2.poll()));
    }

    #[tokio::test]
    async fn sender_fails_on_rx_close() {
        let (tx, rx) = bounded(1);

        let msg = 10;

        // wrap futures
        let mut send1 = spawn(async { tx.send(msg).await });

        drop(rx);
        assert_eq!(Err(SendError::Disconnected), assert_ready!(send1.poll()));
    }
}
