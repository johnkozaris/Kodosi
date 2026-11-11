use std::{io, path::PathBuf, sync::Arc};

use bytes::Bytes;
use tokio::sync::watch;

/// Writes away from the runtime's task, which also carries what people type. One write runs at a
/// time, always with the newest bytes, so an earlier save never overwrites a later one, and the
/// saves that come during a write make one write after it.
pub(super) struct Saver {
    /// The number of the newest save, and its bytes.
    newest: watch::Sender<(u64, Bytes)>,
    /// The number of the last save that was written or failed, and whether it failed.
    done: watch::Receiver<(u64, bool)>,
}

impl Saver {
    pub(super) fn file(file: PathBuf, what: &'static str) -> Self {
        Self::new(what, move |bytes| {
            kodosi_os::fs::write_private(&file, bytes)
        })
    }

    fn new(
        what: &'static str,
        write: impl Fn(&[u8]) -> io::Result<()> + Send + Sync + 'static,
    ) -> Self {
        let (newest, mut next) = watch::channel((0, Bytes::new()));
        let (finished, done) = watch::channel((0, false));
        let write = Arc::new(write);
        tokio::spawn(async move {
            while next.changed().await.is_ok() {
                let (number, bytes) = next.borrow_and_update().clone();
                let write = Arc::clone(&write);
                let written = tokio::task::spawn_blocking(move || write(&bytes))
                    .await
                    .map_err(io::Error::other)
                    .flatten();
                if let Err(error) = &written {
                    tracing::warn!(%error, "could not save {what}");
                }
                finished.send_replace((number, written.is_err()));
            }
        });
        Self { newest, done }
    }

    pub(super) fn save(&self, bytes: Vec<u8>) {
        self.newest.send_modify(|(number, newest)| {
            *number += 1;
            *newest = Bytes::from(bytes);
        });
    }

    #[cfg(feature = "host")]
    pub(super) fn failed(&self) -> bool {
        self.done.borrow().1
    }

    pub(super) async fn saved(&self) {
        let number = self.newest.borrow().0;
        let mut done = self.done.clone();
        drop(done.wait_for(|(done, _)| *done >= number).await);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, mpsc};

    use super::*;

    #[tokio::test]
    async fn the_saves_during_a_write_make_one_write_of_the_newest_after_it() {
        let written = Arc::new(Mutex::new(Vec::new()));
        let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
        let (release, released) = mpsc::channel::<()>();
        let released = Mutex::new(released);
        let saver = Saver::new("numbers", {
            let written = Arc::clone(&written);
            move |bytes: &[u8]| {
                started.send(()).unwrap();
                if bytes == b"1" {
                    released.lock().unwrap().recv().unwrap();
                }
                written.lock().unwrap().push(bytes.to_vec());
                Ok(())
            }
        });
        saver.save(b"1".to_vec());
        starts.recv().await.unwrap();
        for number in 2..=9 {
            saver.save(number.to_string().into_bytes());
        }
        release.send(()).unwrap();
        saver.saved().await;
        assert_eq!(*written.lock().unwrap(), [b"1".to_vec(), b"9".to_vec()]);
    }

    #[cfg(feature = "host")]
    #[tokio::test]
    async fn a_failed_write_is_known_until_a_write_succeeds() {
        let saver = Saver::new("numbers", |bytes: &[u8]| {
            if bytes == b"full" {
                Err(io::Error::other("the disk is full"))
            } else {
                Ok(())
            }
        });
        saver.save(b"full".to_vec());
        saver.saved().await;
        assert!(saver.failed());
        saver.save(b"1".to_vec());
        saver.saved().await;
        assert!(!saver.failed());
    }
}
