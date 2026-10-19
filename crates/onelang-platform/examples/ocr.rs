//! `cargo run -p onelang-platform --example ocr -- image.png [lang]`
//! Without a language the Snipping Tool engine is used (if installed).
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("image path");
    let lang = args.next();
    onelang_platform::oneocr::set_work_dir(std::env::temp_dir().join("onelang-oneocr"));
    println!("installed: {:?}", onelang_platform::ocr::available_languages());
    println!("snipping tool: {}", onelang_platform::oneocr::is_available());
    let img = image::open(&path).expect("open").to_rgba8();
    let t = std::time::Instant::now();
    let out =
        onelang_platform::ocr::recognize(&img, lang.is_none(), lang.as_deref(), &["ru".into(), "en".into(), "es".into()]);
    println!("{:?} in {:?}", out, t.elapsed());
}
