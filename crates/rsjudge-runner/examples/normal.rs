use caps::{read, CapSet};

fn main() {
    dbg!(read(None, CapSet::Ambient).unwrap());
    dbg!(read(None, CapSet::Effective).unwrap());
    dbg!(read(None, CapSet::Inheritable).unwrap());
    dbg!(read(None, CapSet::Permitted).unwrap());
    println!("Hello, world!");
}
