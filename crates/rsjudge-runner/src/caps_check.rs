use std::convert::identity;

use caps::{errors::CapsError, has_cap, Capability};

use crate::error::{Error, Result};

pub fn require_caps<I>(caps: I) -> Result<()>
where
    I: IntoIterator<Item = Capability>,
{
    let missing_caps = caps
        .into_iter()
        .map(|cap| match has_cap(None, caps::CapSet::Effective, cap) {
            Ok(has_cap) => Ok((!has_cap).then_some(cap)),
            Err(e) => Err(e),
        })
        .collect::<Result<Vec<_>, CapsError>>()?
        .into_iter()
        .filter_map(identity)
        .collect::<Vec<_>>();

    if missing_caps.is_empty() {
        Ok(())
    } else {
        Err(Error::CapsRequired {
            caps: missing_caps.into_boxed_slice(),
        })
    }
}
