fn main() {
    println!("cargo:rustc-link-arg=-nostdlib");

    // Rust may emit these libcalls depending on the compiler and optimization
    // level. Resolve them to funchook's hidden freestanding implementations;
    // the cdylib's version script keeps the aliases local to this object.
    for symbol in ["memcpy", "memmove", "memset", "strncpy"] {
        println!("cargo:rustc-link-arg=-Wl,--defsym={symbol}=funchook_{symbol}");
    }
}
