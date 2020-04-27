fn leb128_write(v: &mut Vec<u8>, mut word: u32) {
    for _ in 0..5 {
        let mut byte = (word & 0x7f) as u8;
        word >>= 7;

        if word != 0 {
            byte |= crate::leb128::CONTINUE;
        }
        v.push(byte);

        if word == 0 {
            return;
        }
    }
}

#[test]
fn encode_and_parse() {
    let data = &[1, 2, 3, 4, 5];
    let data_size = data.len();
    let sym = 0xcafe;
    let typ = 0xdeafbeef;

    let mut buf = Vec::new();
    leb128_write(&mut buf, data_size as u32);
    leb128_write(&mut buf, sym);
    leb128_write(&mut buf, typ);
    buf.extend(data.iter());

    let mut parser = crate::parser::Parser::new();

    parser.push(&buf[0..6]);
    let packet = parser.try_parse();
    assert_eq!(packet, None);

    parser.push(&buf[6..12]);
    let packet = parser.try_parse();
    assert_eq!(packet, None);

    parser.push(&buf[12..14]);
    let packet = parser.try_parse();
    assert_eq!(
        packet,
        Some(crate::parser::Packet {
            string_loc: 0xcafe,
            type_loc: 0xdeafbeef,
            buffer: vec![1, 2, 3, 4, 5]
        })
    );
}

#[test]
fn data_to_read() {
    let buf_size = 1024;
    assert_eq!(crate::bytes_to_read(1022, 8, buf_size), 10);
}
