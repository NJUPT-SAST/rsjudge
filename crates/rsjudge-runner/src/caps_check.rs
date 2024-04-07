use caps::{has_cap, raise, Capability};

use crate::error::{Error, Result};

fn raise_cap(cap: Capability) -> Result<bool> {
    if has_cap(None, caps::CapSet::Permitted, cap)? {
        raise(None, caps::CapSet::Effective, cap)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn require_caps<I>(caps: I) -> Result<()>
where
    I: IntoIterator<Item = Capability>,
{
    let missing_caps = caps.into_iter().try_fold(vec![], |s, cap| {
        let mut v: Vec<_> = s;
        if !raise_cap(cap)? {
            v.push(cap);
        }
        Ok::<_, Error>(v)
    })?;

    if !missing_caps.is_empty() {
        Err(Error::CapsRequired {
            caps: missing_caps.into_boxed_slice(),
        })?
    }
    Ok(())
}
