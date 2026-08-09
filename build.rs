use std::process::Command;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "macos" {
        // Motif/Xt/X11/Xft come from Homebrew (XQuartz supplies the X server).
        let prefix = Command::new("brew")
            .arg("--prefix")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "/opt/homebrew".into());
        println!("cargo:rustc-link-search=native={prefix}/lib");
    } else {
        // Linux/other: pkg-config resolves multiarch lib dirs for the X libs;
        // Motif ships no reliable .pc, so add the usual system lib dirs too.
        for lib in ["x11", "xt", "xft"] {
            if let Ok(out) = Command::new("pkg-config")
                .args(["--libs-only-L", lib])
                .output()
            {
                for tok in String::from_utf8_lossy(&out.stdout).split_whitespace() {
                    if let Some(path) = tok.strip_prefix("-L") {
                        println!("cargo:rustc-link-search=native={path}");
                    }
                }
            }
        }
        for dir in ["/usr/lib", "/usr/lib64", "/usr/local/lib"] {
            println!("cargo:rustc-link-search=native={dir}");
        }
    }

    println!("cargo:rustc-link-lib=dylib=Xm");
    println!("cargo:rustc-link-lib=dylib=Xt");
    println!("cargo:rustc-link-lib=dylib=X11");
    println!("cargo:rustc-link-lib=dylib=Xft"); // self-rendered Unicode/emoji text
}
