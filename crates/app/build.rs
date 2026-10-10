// En Windows incrusta el icono y los datos del programa en quantum-daw.exe.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/windows/quantum-daw.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("../../assets/windows/quantum-daw.ico");
        r.set("ProductName", "Quantum DAW");
        r.set("FileDescription", "Quantum DAW");
        r.set("CompanyName", "QUANTEX");
        r.set("LegalCopyright", "MPL-2.0");
        if let Err(e) = r.compile() {
            println!("cargo:warning=No se pudo incrustar el icono de Windows: {e}");
        }
    }
}
