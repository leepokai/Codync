//! Coding agents on one computer, for an app that runs them: which are installed and how to install, start and
//! sign in to each ([`backends`], [`registry`], [`node`]), ACP over stdio ([`acp`]) with what they say as typed
//! [`events`] and the process tree each one starts ([`process`]), their permission modes ([`modes`]), and more than
//! one login per agent ([`accounts`]).
//!
//! The app calls [`init`] once at start, before any agent runs.

pub mod accounts;
pub mod acp;
pub mod backends;
pub mod config;
pub mod events;
pub mod modes;
pub mod node;
pub mod process;
pub mod registry;
mod setup;
pub mod shell;

#[cfg(unix)]
mod process_unix;
#[cfg(windows)]
mod process_windows;

pub use setup::{Setup, init};

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks a std mutex even after a panic poisoned it: what it guards stays usable.
trait LockExt<T> {
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
