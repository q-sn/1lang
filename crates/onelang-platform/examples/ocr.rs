//! `cargo run -p onelang-platform --example ocr -- image.png [lang]`
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("image path");
    let lang = args.next();
    println!("installed: {:?}", onelang_platform::ocr::available_languages());
    let img = image::open(&path).expect("open").to_rgba8();
    let t = std::time::Instant::now();
    let out = onelang_platform::ocr::recognize(&img, lang.as_deref(), &["ru".into(), "en".into(), "es".into()]);
    println!("{:?} in {:?}", out, t.elapsed());
}
