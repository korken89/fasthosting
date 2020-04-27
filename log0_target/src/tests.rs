use std::vec::Vec;

fn leb128_write(v: &mut Vec<u8>, mut word: u32) {
    loop {
        let mut byte = (word & 0x7f) as u8;
        word >>= 7;

        if word != 0 {
            byte |= crate::CONTINUE;
        }
        v.push(byte);

        if word == 0 {
            return;
        }
    }
}

#[test]
fn it_works() {
    let mut v = Vec::new();
    leb128_write(&mut v, 0xffff_ffff);
    println!("{:#?}", v);
    assert_eq!(v.len(), 5);
}
