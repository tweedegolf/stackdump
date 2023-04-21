use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();
    println!("Detected target: {target}");

    let is_cortex_m = target.starts_with("thumbv6m-")
        || target.starts_with("thumbv7m-")
        || target.starts_with("thumbv7em-")
        || target.starts_with("thumbv8m.base")
        || target.starts_with("thumbv8m.main");

    let is_avr = target.starts_with("avr-");

    if is_cortex_m {
        println!("cargo:rustc-cfg=cortex_m");

        if target.ends_with("-eabihf") {
            println!("cargo:rustc-cfg=has_fpu");
        }
    }

    if is_avr {
        println!("cargo:rustc-cfg=avr");
    }
}
