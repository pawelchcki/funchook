use std::env;
use std::path::Path;

fn main() {
    let target = env::var("TARGET").expect("Cargo did not set TARGET");
    let os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo did not set target OS");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo did not set target architecture");
    let pointer_width =
        env::var("CARGO_CFG_TARGET_POINTER_WIDTH").expect("Cargo did not set pointer width");

    let cpu = match arch.as_str() {
        "x86" | "x86_64" => "x86",
        "aarch64" => "arm64",
        _ => unsupported(&target),
    };
    match (os.as_str(), arch.as_str()) {
        ("linux", "x86" | "x86_64" | "aarch64")
        | ("macos", "x86_64" | "aarch64")
        | ("windows", "x86" | "x86_64" | "aarch64") => {}
        _ => unsupported(&target),
    }

    let mut config = cmake::Config::new(".");
    config
        .define("FUNCHOOK_CPU", cpu)
        .define(
            "CMAKE_SIZEOF_VOID_P",
            if pointer_width == "32" { "4" } else { "8" },
        )
        .define("FUNCHOOK_DISASM", "capstone")
        .define("FUNCHOOK_BUILD_SHARED", "OFF")
        .define("FUNCHOOK_BUILD_STATIC", "ON")
        .define("FUNCHOOK_BUILD_TESTS", "OFF")
        .define("FUNCHOOK_INSTALL", "ON")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_STATIC_LIBS", "ON")
        .define("BUILD_STATIC_RUNTIME", "OFF")
        .define("CAPSTONE_BUILD_TESTS", "OFF")
        .define("CAPSTONE_BUILD_CSTOOL", "OFF")
        .define("CAPSTONE_BUILD_CSTEST", "OFF")
        .define("CAPSTONE_BUILD_MACOS_THIN", "ON")
        .define("CAPSTONE_ARCHITECTURE_DEFAULT", "OFF")
        .define("CAPSTONE_INSTALL", "ON")
        .define(
            if cpu == "x86" {
                "CAPSTONE_X86_SUPPORT"
            } else {
                "CAPSTONE_ARM64_SUPPORT"
            },
            "ON",
        )
        .pic(true);

    let dst = config.build();
    let lib = dst.join("lib");
    let lib64 = dst.join("lib64");
    emit_link_search(&lib);
    emit_link_search(&lib64);
    emit_link_search(&dst.join("bin"));

    println!("cargo:rustc-link-lib=static=funchook");
    println!("cargo:rustc-link-lib=static=capstone");
    match os.as_str() {
        "linux" => {
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=c");
        }
        "macos" => println!("cargo:rustc-link-lib=c"),
        "windows" => println!("cargo:rustc-link-lib=psapi"),
        _ => unreachable!(),
    }

    for path in [
        "CMakeLists.txt",
        "include",
        "src",
        "vendor/capstone/CMakeLists.txt",
        "vendor/capstone/include",
        if cpu == "x86" {
            "vendor/capstone/arch/X86"
        } else {
            "vendor/capstone/arch/AArch64"
        },
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn emit_link_search(path: &Path) {
    if path.exists() {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
}

fn unsupported(target: &str) -> ! {
    panic!(
        "funchook does not support target `{target}`; supported targets are Linux x86/x86_64/aarch64, macOS x86_64/aarch64, and Windows x86/x86_64/aarch64"
    )
}
