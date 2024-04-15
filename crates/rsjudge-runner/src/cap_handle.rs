use std::{cell::RefCell, collections::HashMap, rc::Rc};

use caps::{drop as drop_cap, has_cap, raise as raise_cap, Capability};

use crate::{Error, Result};

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CapHandle {
    cap: Capability,
    ref_count: Rc<()>,
}

impl CapHandle {
    thread_local! {
        /// Local capability reference count.
        static LOCAL_CAPS: RefCell<HashMap<Capability,Rc<()>>> = RefCell::new(HashMap::new());
    }

    /// Create a new capability handle.
    ///
    /// The capability will be raised if it is permitted but not effective.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is not permitted,
    pub fn new(cap: Capability) -> Result<Self> {
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
            let _ = drop_cap(None, caps::CapSet::Effective, self.cap);
            Self::LOCAL_CAPS.with_borrow_mut(|local_caps| {
                local_caps.remove(&self.cap);
            });
        }
    }
}

fn try_raise_cap(cap: Capability) -> Result<bool> {
    if has_cap(None, caps::CapSet::Effective, cap)? {
        // Already has cap.
        Ok(true)
    } else if has_cap(None, caps::CapSet::Permitted, cap)? {
        raise_cap(None, caps::CapSet::Effective, cap)?;
        Ok(true)
    } else {
        Err(Error::CapRequired(cap))
    }
}
