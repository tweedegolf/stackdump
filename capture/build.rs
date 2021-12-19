use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();

    let is_cortex_m = target.starts_with("thumbv6m-")
        || target.starts_with("thumbv7m-")
        || target.starts_with("thumbv7em-")
        || target.starts_with("thumbv8m.base")
        || target.starts_with("thumbv8m.main");

    if is_cortex_m {
        println!("cargo:rustc-cfg=cortex_m");

        if target.ends_with("-eabihf") {
            println!("cargo:rustc-cfg=has_fpu");
        }
    }
}
