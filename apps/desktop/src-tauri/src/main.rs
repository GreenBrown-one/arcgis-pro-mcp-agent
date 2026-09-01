fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--uninstall-cleanup")) {
        std::process::exit(arcgis_pro_agent_desktop_lib::cleanup::cleanup_for_uninstall());
    }
    arcgis_pro_agent_desktop_lib::run();
}
