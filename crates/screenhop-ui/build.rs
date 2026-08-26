fn main() {
    slint_build::compile("ui/app.slint").expect("compile Slint UI");

    println!("cargo:rerun-if-changed=assets/screen-hop.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/screen-hop.ico")
            .compile()
            .expect("embed screen-hop Windows icon");
    }
}
