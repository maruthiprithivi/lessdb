//! FIFO writer admission: repeated maintenance must not starve a queued insert.
use less_common::{LessError, Result};
use std::sync::{Condvar, Mutex};
#[derive(Default)]
struct State {
    next: u64,
    serving: u64,
    poisoned: bool,
}
#[derive(Default)]
pub(crate) struct TableGate {
    state: Mutex<State>,
    changed: Condvar,
}
pub(crate) struct Guard<'a>(&'a TableGate);
impl TableGate {
    #[cfg(test)]
    pub fn is_locked(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.next != state.serving
    }
    pub fn lock(&self) -> Result<Guard<'_>> {
        let error = || {
            LessError::Engine("table writer gate failed; close and reopen before writing".into())
        };
        let mut state = self.state.lock().map_err(|_| error())?;
        if state.poisoned {
            return Err(error());
        }
        if state.next == state.serving {
            state.next = 0;
            state.serving = 0;
        }
        let ticket = state.next;
        state.next = state.next.checked_add(1).ok_or_else(error)?;
        while ticket != state.serving {
            state = self.changed.wait(state).map_err(|_| error())?;
            if state.poisoned {
                return Err(error());
            }
        }
        Ok(Guard(self))
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.poisoned |= std::thread::panicking();
            state.serving += 1; // highest issued ticket is u64::MAX-1
            self.0.changed.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn queued_writers_are_served_in_registration_order() {
        let gate = Arc::new(TableGate::default());
        let guard = gate.lock().unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        std::thread::scope(|scope| {
            for i in 0..8 {
                let writer = gate.clone();
                let completed = order.clone();
                scope.spawn(move || {
                    let _guard = writer.lock().unwrap();
                    completed.lock().unwrap().push(i);
                });
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while gate.state.lock().unwrap().next < i + 2 {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "writer did not register"
                    );
                    std::thread::yield_now();
                }
            }
            drop(guard);
        });
        assert_eq!(*order.lock().unwrap(), (0..8).collect::<Vec<_>>());
    }
    #[test]
    fn panic_in_critical_operation_poisoned_gate_fails_closed() {
        let gate = Arc::new(TableGate::default());
        let worker = gate.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = worker.lock().unwrap();
                panic!("injected operation panic");
            })
            .join()
            .is_err()
        );
        assert!(gate.lock().is_err());
    }
}
