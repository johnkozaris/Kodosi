use std::{
    io,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use keyring_core::{Entry, Error, api::CredentialStoreApi as _};
use zbus_secret_service_keyring_store::Store as SecretService;
use zeroize::Zeroizing;

const OPENING: Duration = Duration::from_secs(10);

static SECRET_SERVICE: Session<SecretService> = Session::new();

pub(super) fn available() -> bool {
    secret_service().is_ok()
}

fn secret_service() -> io::Result<Arc<SecretService>> {
    SECRET_SERVICE.get(OPENING, || {
        SecretService::new().map_err(|error| unavailable(&error))
    })
}

pub(super) struct Store {
    service: String,
}

impl Store {
    pub(super) fn new(service: &str, _directory: &Path) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    fn entry(&self, name: &str) -> io::Result<(Arc<SecretService>, Entry)> {
        let session = secret_service()?;
        let entry = session
            .build(&self.service, name, None)
            .map_err(|error| unavailable(&error))?;
        Ok((session, entry))
    }

    pub(super) fn load(&self, name: &str) -> io::Result<Option<Zeroizing<String>>> {
        let (session, entry) = self.entry(name)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(failed(&session, "Secret Service is unavailable", &error)),
        }
    }

    pub(super) fn store(&self, name: &str, value: &str) -> io::Result<()> {
        let (session, entry) = self.entry(name)?;
        entry.set_password(value).map_err(|error| {
            failed(
                &session,
                "Could not store the credential in Secret Service",
                &error,
            )
        })
    }

    pub(super) fn delete(&self, name: &str) -> io::Result<()> {
        let (session, entry) = self.entry(name)?;
        match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(failed(
                &session,
                "Could not remove the credential from Secret Service",
                &error,
            )),
        }
    }
}

fn unavailable(error: &dyn std::fmt::Display) -> io::Error {
    io::Error::other(format!("Secret Service is unavailable: {error}"))
}

fn failed(session: &Arc<SecretService>, what: &str, error: &Error) -> io::Error {
    if matches!(error, Error::PlatformFailure(_)) {
        SECRET_SERVICE.close(session);
    }
    io::Error::other(format!("{what}: {error}"))
}

struct Session<T> {
    state: Mutex<State<T>>,
}

enum State<T> {
    Closed,
    Opening(mpsc::Receiver<io::Result<Arc<T>>>),
    Open(Arc<T>),
}

impl<T: Send + Sync + 'static> Session<T> {
    const fn new() -> Self {
        Self {
            state: Mutex::new(State::Closed),
        }
    }

    fn get(
        &self,
        wait: Duration,
        open: impl FnOnce() -> io::Result<Arc<T>> + Send + 'static,
    ) -> io::Result<Arc<T>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("Secret Service lock failed"))?;
        let opening = match std::mem::replace(&mut *state, State::Closed) {
            State::Open(session) => {
                *state = State::Open(Arc::clone(&session));
                return Ok(session);
            }
            State::Opening(opening) => opening,
            State::Closed => {
                let (answer, opening) = mpsc::sync_channel(1);
                thread::Builder::new()
                    .name("kodosi-secret-service".into())
                    .spawn(move || drop(answer.send(open())))?;
                opening
            }
        };
        let (next, session) = match opening.recv_timeout(wait) {
            Ok(Ok(session)) => (State::Open(Arc::clone(&session)), Ok(session)),
            Ok(Err(error)) => (State::Closed, Err(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => (
                State::Opening(opening),
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Secret Service is unavailable: it did not answer in time",
                )),
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) => (
                State::Closed,
                Err(io::Error::other(
                    "Secret Service is unavailable: its session did not open",
                )),
            ),
        };
        *state = next;
        drop(state);
        session
    }

    fn close(&self, failed: &Arc<T>) {
        if let Ok(mut state) = self.state.lock()
            && matches!(&*state, State::Open(session) if Arc::ptr_eq(session, failed))
        {
            *state = State::Closed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_opening_is_tried_again() {
        let session = Session::<u32>::new();
        assert!(
            session
                .get(OPENING, || Err(io::Error::other("not yet")))
                .is_err()
        );
        assert_eq!(*session.get(OPENING, || Ok(Arc::new(7))).unwrap(), 7);
        assert_eq!(*session.get(OPENING, || unreachable!()).unwrap(), 7);
    }

    #[test]
    fn a_slow_opening_is_waited_for_and_not_started_twice() {
        let session = Session::<u32>::new();
        let slow = session.get(Duration::from_millis(20), || {
            thread::sleep(Duration::from_millis(200));
            Ok(Arc::new(5))
        });
        assert_eq!(slow.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(*session.get(OPENING, || unreachable!()).unwrap(), 5);
    }

    #[test]
    fn a_lost_session_opens_again() {
        let session = Session::<u32>::new();
        let first = session.get(OPENING, || Ok(Arc::new(1))).unwrap();
        session.close(&Arc::new(1));
        assert!(Arc::ptr_eq(
            &session.get(OPENING, || unreachable!()).unwrap(),
            &first
        ));
        session.close(&first);
        assert_eq!(*session.get(OPENING, || Ok(Arc::new(2))).unwrap(), 2);
    }
}
