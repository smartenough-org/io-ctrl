fn main() {
    println!("cargo::rustc-link-arg-bins=--nmagic");
    println!("cargo::rustc-link-arg-bins=-Tlink.x");
    println!("cargo::rustc-link-arg-bins=-Tdefmt.x");
    // Tests.
    println!("cargo::rustc-link-arg-tests=--nmagic");
    println!("cargo::rustc-link-arg-tests=-Tlink.x");
    println!("cargo::rustc-link-arg-tests=-Tembedded-test.x");
    println!("cargo::rustc-link-arg-tests=-Tdefmt.x");
}
