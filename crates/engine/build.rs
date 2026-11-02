//! Con QUANTUM_LILV_ESTATICO=<prefijo> (paquetes de Linux, ver packaging/linux/lilv.sh) lilv y sus
//! dependencias quedan dentro del binario: funciona aunque el sistema no tenga lilv instalado.
fn main() {
    println!("cargo:rerun-if-env-changed=QUANTUM_LILV_ESTATICO");
    if let Ok(prefijo) = std::env::var("QUANTUM_LILV_ESTATICO")
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
    {
        println!("cargo:rustc-link-search=native={prefijo}/lib");
        // En orden: cada una usa las que le siguen.
        for lib in ["lilv-0", "sratom-0", "sord-0", "serd-0", "zix-0"] {
            println!("cargo:rustc-link-lib=static={lib}");
        }
        println!("cargo:rustc-link-lib=dylib=dl");
        println!("cargo:rustc-link-lib=dylib=m");
    }
}
