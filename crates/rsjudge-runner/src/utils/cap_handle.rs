// SPDX-License-Identifier: Apache-2.0

//! RAII-style Capability handle.
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

pub use capctl::Cap;
use capctl::CapState;
use rsjudge_utils::log_if_error;

use crate::{Result, error::CapRequiredError};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct NonCopy;

/// An RAII-style handle for capabilities.
///
/// When constructed, the handle will raise the capability if it is permitted but not effective.
///
/// When dropped, the handle will drop the capability if it is the last reference.
///
/// # Note
///
/// You can use [`use_caps!`][crate::use_caps] macro to simplify the usage of this struct.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CapHandle {
    cap: Cap,
    rc: Rc<NonCopy>,
}

impl CapHandle {
    thread_local! {
        /// Local capability reference count.
        static LOCAL_CAPS: RefCell<HashMap<Cap, Weak<NonCopy>>> = RefCell::new(HashMap::new());
    }

    /// Create a new capability handle.
    ///
    /// The capability will be raised if it is permitted but not effective.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is not permitted,
    pub fn new(cap: Cap) -> Result<Self> {
        Self::LOCAL_CAPS.with_borrow_mut(|local_caps| {
            local_caps
                .get(&cap)
                .and_then(Weak::upgrade)
                .map(|rc| Self { cap, rc })
                .map_or_else(
                    || {
                        try_raise_cap(cap)?;
                        let rc = Rc::new_cyclic(|weak| {
                            local_caps.insert(cap, weak.clone());
                            NonCopy
                        });

                        Ok(Self { cap, rc })
                    },
                    Ok,
                )
        })
    }
}

impl Drop for CapHandle {
    #[allow(clippy::print_stderr)]
    fn drop(&mut self) {
        if Rc::strong_count(&self.rc) == 1 {
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
        Err(CapRequiredError(cap))?
    }
}
