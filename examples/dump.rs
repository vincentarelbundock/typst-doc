fn main() {
    let src = std::env::args().nth(1).expect("usage: dump <rd source>");
    let doc = rd_source::parse(src.as_bytes())
        .expect("parses")
        .into_parts()
        .0;
    println!("{:#?}", doc.nodes());
}
