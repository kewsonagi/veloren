use clap::{App, Arg};
use veloren_voxygen_i18n::{analysis, verification, BasePath};

fn main() {
    let matches = App::new("i18n-check")
        .version("0.1.0")
        .author("juliancoffee <lightdarkdaughter@gmail.com>")
        .about("Test veloren localizations")
        .arg(
            Arg::with_name("CODE")
                .required(false)
                .help("Run diagnostic for specific language code (de_DE as example)"),
        )
        .arg(
            Arg::with_name("verify")
                .long("verify")
                .help("verify all localizations"),
        )
        .arg(
            Arg::with_name("test")
                .long("test")
                .help("test all localizations"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("print additional information"),
        )
        .arg(
            Arg::with_name("csv")
                .long("csv")
                .help("generate csv files per language in target folder"),
        )
        .get_matches();

    // Generate paths
    let root_path = common_assets::find_root().expect("Failed to find root of repository");
    let path = BasePath::new(&root_path);
    let be_verbose = matches.is_present("verbose");
    let csv_enabled = matches.is_present("csv");

    if let Some(code) = matches.value_of("CODE") {
        analysis::test_specific_localizations(&path, &[code], be_verbose, csv_enabled);
    }
    if matches.is_present("test") {
        analysis::test_all_localizations(&path, be_verbose, csv_enabled);
    }
    if matches.is_present("verify") {
        verification::verify_all_localizations(&path);
    }
}
