fn main() {
    noi_build::BuildOptions::from_env("noir")
        .and_then(|opts| opts.export())
        .unwrap_or_else(|err| panic!("failed to export Noir artifacts: {err:#}"));
}
