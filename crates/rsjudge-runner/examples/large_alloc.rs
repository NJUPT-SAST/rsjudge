// SPDX-License-Identifier: Apache-2.0

fn main() {
    let mut v: Vec<u32> = vec![0];
    loop {
        v.extend_from_within(..);
    }
}
