use std::{env, fs};

fn main() {
    // slint_build::compile("ui/appwindow.slint").unwrap();

    let result = slint_build::compile("ui/appwindow.slint");
    if result.is_ok() {
        /*
         This is actually a bug in the documentation of slint_build::compile,
         it's compiled to the "OUT_DIR" environment variable,
         not the "OUT" environment variable.
        */
        if let Ok(out_dir) = env::var("OUT_DIR") {
            let ui_file = format!("{out_dir}/appwindow.rs");
            if let Ok(mut ui_file_content) = fs::read_to_string(ui_file.clone()) {
                // Insert allows clippy-pedantic in the first line.
                ui_file_content.insert_str(
                    0,
                    "#[allow(clippy::all, clippy::pedantic, clippy::nursery)]\n",
                );
                fs::write(ui_file, ui_file_content).unwrap();
            } else {
                panic!("Error reading UI file: {ui_file}");
            }
        } else {
            panic!("Error reading OUT_DIR environment variable during build, for output dir.");
        }
    } else {
        panic!(
            "Error building Slint UI files. Error: {:?}",
            result.err().unwrap()
        );
    }
}
