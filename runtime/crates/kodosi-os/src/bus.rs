use std::{sync::mpsc, thread, time::Duration};

use zbus::{Connection, connection::Builder};

const LIMIT: Duration = Duration::from_secs(3);

pub(crate) fn ask_session<T: Send + 'static>(
    question: impl AsyncFnOnce(&Connection) -> Option<T> + Send + 'static,
) -> Option<T> {
    ask(Builder::session, question)
}

pub(crate) fn ask_system<T: Send + 'static>(
    question: impl AsyncFnOnce(&Connection) -> Option<T> + Send + 'static,
) -> Option<T> {
    ask(Builder::system, question)
}

/// Asks again on the connection of an earlier question.
pub(crate) fn ask_on<T: Send + 'static>(
    connection: Connection,
    question: impl AsyncFnOnce(&Connection) -> Option<T> + Send + 'static,
) -> Option<T> {
    within(LIMIT, move || zbus::block_on(question(&connection)))
}

fn ask<T: Send + 'static>(
    bus: fn() -> Builder<'static>,
    question: impl AsyncFnOnce(&Connection) -> Option<T> + Send + 'static,
) -> Option<T> {
    within(LIMIT, move || {
        zbus::block_on(async move {
            let connection = bus().method_timeout(LIMIT).build().await.ok()?;
            question(&connection).await
        })
    })
}

fn within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    let (answer, answered) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("kodosi-bus".into())
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
            zbus::block_on(async move {
                Builder::address(address.as_str())
                    .build()
                    .await
                    .ok()
                    .map(drop)
            })
        });
        assert!(answer.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(listener);
    }
}
