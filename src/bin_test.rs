fn main() {
    let enc = encoding_rs::Encoding::for_label(b"iso-8859-1");
    println!("for_label iso-8859-1: {:?}", enc.map(|e| e.name()));
    let enc2 = encoding_rs::Encoding::for_label(b"windows-1252");
    println!("for_label windows-1252: {:?}", enc2.map(|e| e.name()));
}
