use std::{sync::mpsc, thread, time::Duration};

use zbus::blocking::{Connection, connection::Builder};

const LIMIT: Duration = Duration::from_secs(3);

/// Asks the session bus and gives up after a few seconds.
pub(crate) fn ask<T: Send + 'static>(
    question: impl FnOnce(&Connection) -> Option<T> + Send + 'static,
) -> Option<T> {
    within(LIMIT, move || {
        let connection = Builder::session()
            .ok()?
            .method_timeout(LIMIT)
            .build()
            .ok()?;
        question(&connection)
    })
}

/// Runs work that talks to the session bus on a thread of its own and gives up after
/// `limit`. zbus limits method calls but not the connection handshake, so a bus that
/// accepts a connection and never answers would otherwise block the caller for good.
pub(crate) fn within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    let (answer, answered) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("kodosi-session-bus".into())
        .spawn(move || drop(answer.send(work())))
        .ok()?;
    answered.recv_timeout(limit).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bus_that_never_answers_does_not_block_the_caller() {
        let folder = tempfile::tempdir().unwrap();
        let socket = folder.path().join("bus.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let address = format!("unix:path={}", socket.display());
        let started = std::time::Instant::now();
        let answer = within(Duration::from_millis(300), move || {
            Builder::address(address.as_str())
                .ok()?
                .build()
                .ok()
                .map(drop)
        });
        assert!(answer.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(listener);
    }

    #[test]
    fn a_quick_answer_comes_back() {
        assert_eq!(within(LIMIT, || Some(7)), Some(7));
    }
}
