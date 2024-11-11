use std::cell::UnsafeCell;

use crate::{System, SystemId, SystemModify, SystemNode, SystemVersion};

impl System {
    pub fn var<T>(&self, value: T) -> Var<T> {
        Var::new(self, value)
    }
}

pub struct Var<T: ?Sized> {
    system_id: SystemId,
    value: UnsafeCell<(SystemVersion, T)>,
}

unsafe impl<T: Sync + Send + ?Sized> Sync for Var<T> {}

impl<T> Var<T> {
    ///
    /// ```
    /// # use lazy_catch::System;
    /// # use lazy_catch::var::Var;
    /// let system = System::new();
    /// let x = Var::new(&system, 0);
    /// ```
    pub fn new(system: &System, value: T) -> Self {
        Self {
            system_id: system.id(),
            value: UnsafeCell::new((system.version(), value)),
        }
    }
}

impl<T: ?Sized> Var<T> {
    pub fn modify<'s>(&'s self, modify: &'s mut SystemModify) -> &'s mut T {
        self.system_id.check_modify(modify);
        let (version, value) = unsafe { &mut *self.value.get() };
        *version = modify.version();
        value
    }
}

impl<T: ?Sized> SystemNode for Var<T> {
    type Value = T;

    fn get_value<'s>(&'s self, system: &'s System) -> (SystemVersion, &'s Self::Value) {
        self.system_id.check_system(system);
        let (version, value) = unsafe { &*self.value.get() };
        (*version, value)
    }
}
