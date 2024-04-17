// SPDX-License-Identifier: Apache-2.0

//! RAII-style Capability handle.
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub use capctl::Cap;
use capctl::CapState;
use rsjudge_utils::log_if_error;

use crate::{Error, Result};
/// An RAII-style handle for capabilities.
///
/// When constructed, the handle will raise the capability if it is permitted but not effective.
///
/// When dropped, the handle will drop the capability if it is the last reference.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CapHandle {
    cap: Cap,
    ref_count: Rc<()>,
}

impl CapHandle {
    thread_local! {
        /// Local capability reference count.
        static LOCAL_CAPS: RefCell<HashMap<Cap, Rc<()>>> = RefCell::new(HashMap::new());
    }

    /// Create a new capability handle.
    ///
    /// The capability will be raised if it is permitted but not effective.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is not permitted,
    pub fn new(cap: Cap) -> Result<Self> {
        let ref_count = Self::LOCAL_CAPS
            .with_borrow_mut(|local_caps| local_caps.entry(cap).or_default().clone());
        try_raise_cap(cap)?;
        Ok(Self { cap, ref_count })
    }
}

impl Drop for CapHandle {
    fn drop(&mut self) {
        if Rc::strong_count(&self.ref_count) == 1 {
            // Last reference.

            let cap = self.cap;

            // We cannot throw errors in `drop`, so we just log and ignore it.
            let _ = log_if_error!(CapState::get_current().and_then(|mut state| {
                state.effective.drop(cap);
                state.set_current()
            }));

            Self::LOCAL_CAPS.with_borrow_mut(|local_caps| {
                local_caps.remove(&self.cap);
            });
        }
    }
}

fn try_raise_cap(cap: Cap) -> Result<()> {
    assert!(cap.is_supported());
    let mut state = CapState::get_current()?;
    if state.effective.has(cap) {
        // Already has cap.
        Ok(())
    } else if state.permitted.has(cap) {
        state.effective.add(cap);
        state.set_current()?;
        Ok(())
    } else {
        Err(Error::CapRequired(cap))
    }
}
