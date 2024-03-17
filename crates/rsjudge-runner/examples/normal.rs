use caps::{read, CapSet};

fn main() {
    eprintln!("Start normal binary");
    dbg!(read(None, CapSet::Ambient).unwrap());
    dbg!(read(None, CapSet::Effective).unwrap());
    dbg!(read(None, CapSet::Inheritable).unwrap());
    dbg!(read(None, CapSet::Permitted).unwrap());
    eprintln!("End normal binary");
}
