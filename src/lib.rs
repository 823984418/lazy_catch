//!
//! ```
//! # use lazy_catch::{System, Update};
//!
//! let mut system = System::new();
//!
//! let arc_x = std::sync::Arc::new(system.var(0));
//!
//! assert_eq!(*system.get(&*arc_x), 0);
//!
//! let a = system.val(|mut u: Update<i32>| {
//!     let v = *u.get(&*arc_x);
//!     u.update(|| v + 1);
//! });
//! assert_eq!(*system.get(&a), 1);
//!
//! let arc_x_clone = arc_x.clone();
//! let b = system.sync_val(move |mut u: Update<i32>| {
//!     let v = *u.get(&*arc_x_clone);
//!     u.update(|| v + 2);
//! });
//! let mut modify = system.modify();
//! *arc_x.modify(&mut modify) = 10;
//!
//! std::thread::spawn(move || {
//!     assert_eq!(*system.get(&b), 12);
//! }).join().unwrap();
//! ```
//!

pub mod val;
pub mod var;

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// ```
/// # use lazy_catch::System;
///
/// let system = System::new();
/// let system_id = system.id();
/// assert_eq!(system_id, system.id());
/// ```
#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub struct SystemId {
    id: u64,
}

impl SystemId {
    pub(crate) fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let id = ID.fetch_add(1, Ordering::Relaxed);
        Self { id }
    }

    pub fn check_system(&self, system: &System) {
        assert_eq!(*self, system.id());
    }

    pub fn check_modify(&self, modify: &SystemModify) {
        assert_eq!(*self, modify.id());
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct SystemVersion {
    pub(crate) version: NonZeroU64,
}

impl SystemVersion {
    pub(crate) fn new() -> Self {
        Self {
            version: NonZeroU64::new(1).unwrap(),
        }
    }

    pub(crate) fn inc(&mut self) {
        self.version = self.version.checked_add(1).unwrap();
    }
}

#[derive(Debug)]
pub struct System {
    id: SystemId,
    version: SystemVersion,
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

impl System {
    ///
    /// ```
    /// # use lazy_catch::System;
    ///
    /// # fn main() {
    /// let system = System::new();
    /// # }
    /// ```
    ///
    pub fn new() -> Self {
        Self {
            id: SystemId::new(),
            version: SystemVersion::new(),
        }
    }

    pub fn id(&self) -> SystemId {
        self.id
    }

    pub fn version(&self) -> SystemVersion {
        self.version
    }

    pub fn get<'s, N: SystemNode + ?Sized>(&'s self, node: &'s N) -> &'s N::Value {
        let (_version, value) = node.get_value(self);
        value
    }

    pub fn modify(&mut self) -> SystemModify {
        self.version.inc();
        SystemModify { system: self }
    }
}

pub trait SystemNode {
    type Value: ?Sized;

    fn get_value<'s>(&'s self, system: &'s System) -> (SystemVersion, &'s Self::Value);
}

#[derive(Debug)]
pub struct SystemModify<'s> {
    system: &'s mut System,
}

impl<'s> SystemModify<'s> {
    pub fn id(&self) -> SystemId {
        self.system.id()
    }

    pub fn version(&self) -> SystemVersion {
        self.system.version()
    }
}

pub struct Update<'s, T> {
    system: &'s System,
    current_version: Option<SystemVersion>,
    update_version: Option<SystemVersion>,
    receiver: &'s mut Option<(SystemVersion, T)>,
}

impl<'s, T> Update<'s, T> {
    pub fn new(
        system: &'s System,
        current_version: Option<SystemVersion>,
        receiver: &'s mut Option<(SystemVersion, T)>,
    ) -> Self {
        Self {
            system,
            current_version,
            update_version: None,
            receiver,
        }
    }

    pub fn system(&self) -> &'s System {
        self.system
    }

    pub fn get<'r, N: SystemNode + ?Sized>(&'r mut self, node: &'r N) -> &'r N::Value {
        let (version, value) = node.get_value(self.system);
        if let Some(old) = self.update_version {
            if old < version {
                self.update_version = Some(version);
            }
        } else {
            self.update_version = Some(version);
        }
        value
    }

    pub fn update<F: FnOnce() -> T>(self, f: F) {
        let update_version = self.update_version.unwrap_or(self.system().version());
        if let Some(current_version) = self.current_version {
            if update_version <= current_version {
                return;
            }
        }
        *self.receiver = Some((update_version, f()));
    }

    pub fn update_with_old<F: FnOnce(Option<T>) -> T>(self, f: F) {
        let update_version = self.update_version.unwrap_or(self.system().version());
        if let Some(current_version) = self.current_version {
            if update_version <= current_version {
                return;
            }
        }
        let old = self.receiver.take().map(|(_, v)| v);
        *self.receiver = Some((update_version, f(old)));
    }
}
