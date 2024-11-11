use std::cell::{Cell, UnsafeCell};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::{System, SystemId, SystemNode, SystemVersion, Update};

impl System {
    pub fn val<T, F: FnMut(Update<T>)>(&self, f: F) -> Val<T, F> {
        Val::new(self, f)
    }
}

pub struct Val<T, F = fn(Update<T>)> {
    system_id: SystemId,
    check_version: Cell<Option<SystemVersion>>,
    lock: Cell<bool>,
    value: UnsafeCell<(F, Option<(SystemVersion, T)>)>,
}

impl<T, F: FnMut(Update<T>)> Val<T, F> {
    pub fn new(system: &System, f: F) -> Self {
        Self {
            system_id: system.id(),
            check_version: Cell::new(None),
            lock: Cell::new(false),
            value: UnsafeCell::new((f, None)),
        }
    }
}

impl<T, F: FnMut(Update<T>)> SystemNode for Val<T, F> {
    type Value = T;

    fn get_value<'s>(&'s self, system: &'s System) -> (SystemVersion, &'s Self::Value) {
        self.system_id.check_system(system);
        if self.check_version.get() != Some(system.version()) {
            if self.lock.get() {
                panic!("Val update recursion");
            }
            self.lock.set(true);
            let (update_fn, value) = unsafe { &mut *self.value.get() };
            update_fn(Update::new(system, value.as_ref().map(|&(v, _)| v), value));
            self.check_version.set(Some(system.version()));
            self.lock.set(false);
        }
        let (version, value) = unsafe { &*self.value.get() }.1.as_ref().unwrap();
        (*version, value)
    }
}

struct AtomicOptionVersion {
    inner: AtomicU64,
}

impl AtomicOptionVersion {
    fn new() -> Self {
        Self {
            inner: AtomicU64::new(0),
        }
    }

    fn get(&self) -> Option<SystemVersion> {
        NonZeroU64::new(self.inner.load(Ordering::Acquire)).map(|version| SystemVersion { version })
    }

    fn set(&self, v: Option<SystemVersion>) {
        self.inner
            .store(v.map(|s| s.version.get()).unwrap_or(0), Ordering::Release);
    }
}

impl System {
    pub fn sync_val<T, F: FnMut(Update<T>)>(&self, f: F) -> SyncVal<T, F> {
        SyncVal::new(self, f)
    }
}

pub struct SyncVal<T, F = fn(Update<T>)> {
    system_id: SystemId,
    check_version: AtomicOptionVersion,
    lock: Mutex<()>,
    value: UnsafeCell<(F, Option<(SystemVersion, T)>)>,
}

impl<T, F: FnMut(Update<T>)> SyncVal<T, F> {
    pub fn new(system: &System, f: F) -> Self {
        Self {
            system_id: system.id(),
            check_version: AtomicOptionVersion::new(),
            lock: Mutex::new(()),
            value: UnsafeCell::new((f, None)),
        }
    }
}
unsafe impl<T: Sync + Send, F: Send> Sync for SyncVal<T, F> {}

impl<T, F: FnMut(Update<T>)> SystemNode for SyncVal<T, F> {
    type Value = T;

    fn get_value<'s>(&'s self, system: &'s System) -> (SystemVersion, &'s Self::Value) {
        self.system_id.check_system(system);
        if self.check_version.get() != Some(system.version()) {
            let lock = self
                .lock
                .try_lock()
                .expect("Val update recursion or poison");
            if self.check_version.get() != Some(system.version()) {
                let (update_fn, value) = unsafe { &mut *self.value.get() };
                update_fn(Update::new(system, value.as_ref().map(|&(v, _)| v), value));
                self.check_version.set(Some(system.version()));
            }
            drop(lock);
        }
        let (version, value) = unsafe { &*self.value.get() }.1.as_ref().unwrap();
        (*version, value)
    }
}
