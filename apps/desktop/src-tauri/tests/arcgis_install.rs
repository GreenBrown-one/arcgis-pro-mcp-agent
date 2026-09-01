use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use arcgis_pro_agent_desktop_lib::addin_install::{
    ADDIN_PACKAGE_NAME, ADDIN_UNINSTALL_GUIDANCE, AddInInstallError, ArcGisProcessState,
    arcgis_process_state_with, open_packaged_addin_with, packaged_addin_path, uninstall_guidance,
};
use arcgis_pro_agent_desktop_lib::arcgis_install::{
    ArcGisInstallError, ArcGisInstallSource, ArcGisInstallation, arcgis_launch_command,
    choose_arcgis_executable_with, choose_installation, select_sourced_installation,
    validate_installation, validate_installation_with,
};

struct TestDir(tempfile::TempDir);

impl TestDir {
    fn new() -> Self {
        Self(
            tempfile::Builder::new()
                .prefix("arcgis-pro-agent-install-test-")
                .tempdir()
                .expect("create exclusive test directory"),
        )
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

#[test]
fn discovery_prefers_valid_saved_then_registry_then_standard_candidates() {
    let saved = PathBuf::from(r"C:\saved\ArcGIS\Pro");
    let registry = PathBuf::from(r"C:\registry\ArcGIS\Pro");
    let standard = PathBuf::from(r"C:\Program Files\ArcGIS\Pro");

    let chosen = choose_installation(
        [
            saved,
            registry.clone(),
            standard,
            PathBuf::from(r"D:\arcgis_pro"),
        ],
        |root| root == registry,
    )
    .expect("the valid registry candidate should be selected");

    assert_eq!(chosen.root, registry);
}

#[test]
fn sourced_discovery_keeps_labels_when_optional_candidates_are_missing() {
    let standard = PathBuf::from(r"C:\Program Files\ArcGIS\Pro");
    let compatible = PathBuf::from(r"D:\arcgis_pro");
    let selected = select_sourced_installation(
        [
            (ArcGisInstallSource::Standard, standard),
            (ArcGisInstallSource::Compatible, compatible.clone()),
        ],
        |root| {
            if root == compatible {
                Ok(ArcGisInstallation {
                    root: root.to_path_buf(),
                    executable: root.join("bin").join("ArcGISPro.exe"),
                    version: Some("3.7.0.0".to_owned()),
                    source: ArcGisInstallSource::Manual,
                })
            } else {
                Err(ArcGisInstallError::InvalidInstallation)
            }
        },
    )
    .expect("select compatibility candidate");

    assert_eq!(selected.root, compatible);
    assert_eq!(selected.source, ArcGisInstallSource::Compatible);
}

#[test]
fn a_directory_name_without_required_files_is_rejected() {
    let root = TestDir::new();
    fs::create_dir_all(root.path().join("bin")).expect("create bin directory");

    assert!(validate_installation(root.path()).is_err());
}

#[test]
fn validation_accepts_only_arcgis_pro_37_file_versions() {
    let root = TestDir::new();
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).expect("create bin directory");
    fs::write(bin.join("ArcGISPro.exe"), b"test executable").expect("write executable");
    fs::write(bin.join("ArcGIS.Core.dll"), b"test core").expect("write core library");

    let accepted = validate_installation_with(root.path(), |_| Some("3.7.2.15".to_owned()))
        .expect("ArcGIS Pro 3.7 should be accepted");
    assert_eq!(accepted.version.as_deref(), Some("3.7.2.15"));
    assert!(validate_installation_with(root.path(), |_| Some("3.6.5".to_owned())).is_err());
    assert!(validate_installation_with(root.path(), |_| None).is_err());
}

#[test]
fn opens_only_the_exact_bundled_addin_through_the_registered_association() {
    let test = TestDir::new();
    let resource_dir = test.path().join("resources");
    let expected = resource_dir
        .join("generated")
        .join("preview")
        .join(ADDIN_PACKAGE_NAME);
    fs::create_dir_all(expected.parent().unwrap()).expect("create resource directory");
    fs::write(&expected, b"bundled Add-In").expect("write bundled Add-In");
    fs::write(resource_dir.join("user-picked.esriAddinX"), b"unexpected")
        .expect("write unexpected package");
    let mut opened = None;

    let result = open_packaged_addin_with(&resource_dir, ArcGisProcessState::NotRunning, |path| {
        opened = Some(path.to_path_buf());
        Ok(())
    })
    .expect("open bundled package through its registered association");

    let canonical_expected = fs::canonicalize(&expected).expect("canonical bundled package");
    assert_eq!(opened.as_deref(), Some(canonical_expected.as_path()));
    assert_eq!(result.package_name, ADDIN_PACKAGE_NAME);
    assert!(!result.requires_restart);
}

#[test]
fn bundled_addin_resolution_rejects_missing_non_file_and_unexpected_resources() {
    let test = TestDir::new();
    let resource_dir = test.path().join("resources");
    fs::create_dir_all(&resource_dir).expect("create resource directory");
    fs::write(resource_dir.join("other.esriAddinX"), b"unexpected")
        .expect("write unexpected resource");
    assert_eq!(
        packaged_addin_path(&resource_dir),
        Err(AddInInstallError::Unavailable)
    );

    fs::create_dir(resource_dir.join(ADDIN_PACKAGE_NAME)).expect("create package-shaped directory");
    assert_eq!(
        packaged_addin_path(&resource_dir),
        Err(AddInInstallError::Unavailable)
    );
}

#[cfg(windows)]
#[test]
#[ignore = "requires Windows symbolic-link creation privilege"]
fn bundled_addin_resolution_rejects_a_direct_file_symlink_with_a_redacted_error() {
    let test = TestDir::new();
    let resource_dir = test.path().join("resources");
    fs::create_dir_all(&resource_dir).expect("create resource directory");
    let outside = test.path().join("outside.esriAddinX");
    fs::write(&outside, b"outside package").expect("write outside package");
    create_file_symlink(&resource_dir.join(ADDIN_PACKAGE_NAME), &outside);

    let error = packaged_addin_path(&resource_dir).expect_err("reject direct file symlink");

    assert_eq!(error, AddInInstallError::Unavailable);
    assert_eq!(
        error.to_string(),
        "Bundled ArcGIS Pro Add-In is unavailable"
    );
    assert!(
        !error
            .to_string()
            .contains(test.path().to_string_lossy().as_ref())
    );
}

#[cfg(windows)]
#[test]
fn bundled_addin_resolution_rejects_an_ancestor_directory_junction() {
    let test = TestDir::new();
    let resource_dir = test.path().join("resources");
    let outside_generated = test.path().join("outside-generated");
    let outside_package = outside_generated.join("preview").join(ADDIN_PACKAGE_NAME);
    fs::create_dir_all(outside_package.parent().unwrap()).expect("create outside resource layout");
    fs::write(&outside_package, b"outside package").expect("write outside package");
    fs::create_dir_all(&resource_dir).expect("create resource directory");
    create_directory_junction(&resource_dir.join("generated"), &outside_generated);

    let error = packaged_addin_path(&resource_dir).expect_err("reject ancestor junction");

    assert_eq!(error, AddInInstallError::Unavailable);
    assert_eq!(fs::read(outside_package).unwrap(), b"outside package");
}

#[cfg(windows)]
#[test]
fn bundled_addin_resolution_rejects_a_resource_root_directory_junction() {
    let test = TestDir::new();
    let resource_dir = test.path().join("resources");
    let outside_resources = test.path().join("outside-resources");
    let outside_package = outside_resources.join(ADDIN_PACKAGE_NAME);
    fs::create_dir_all(&outside_resources).expect("create outside resource directory");
    fs::write(&outside_package, b"outside package").expect("write outside package");
    create_directory_junction(&resource_dir, &outside_resources);
    let mut opener_called = false;

    let result = open_packaged_addin_with(&resource_dir, ArcGisProcessState::NotRunning, |_| {
        opener_called = true;
        Ok(())
    });

    assert_eq!(result, Err(AddInInstallError::Unavailable));
    assert!(
        !opener_called,
        "the opener must not receive an outside package"
    );
    assert_eq!(fs::read(outside_package).unwrap(), b"outside package");
}

#[test]
fn installer_open_failure_is_safe_and_unknown_process_state_requires_restart_guidance() {
    let test = TestDir::new();
    let package = test.path().join(ADDIN_PACKAGE_NAME);
    fs::write(&package, b"bundled Add-In").expect("write bundled Add-In");
    assert_eq!(
        open_packaged_addin_with(test.path(), ArcGisProcessState::NotRunning, |_| {
            Err(AddInInstallError::Unavailable)
        }),
        Err(AddInInstallError::Unavailable)
    );

    let opened = open_packaged_addin_with(test.path(), ArcGisProcessState::Unknown, |_| Ok(()))
        .expect("open package with unknown process state");
    assert!(opened.requires_restart);
}

#[test]
fn uninstall_returns_addin_manager_guidance_without_a_delete_operation() {
    assert_eq!(uninstall_guidance(), ADDIN_UNINSTALL_GUIDANCE);
    assert!(ADDIN_UNINSTALL_GUIDANCE.contains("Add-In Manager"));
    assert!(ADDIN_UNINSTALL_GUIDANCE.contains("Delete this Add-In"));
}

#[test]
fn production_addin_flow_has_no_direct_addins_mutation_or_private_ownership_protocol() {
    let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let production_sources = production_rust_sources(&source_root);

    assert!(
        !production_sources.is_empty(),
        "expected production Rust sources under {}",
        source_root.display()
    );

    for forbidden in [
        "AddIns",
        "ownership_manifest",
        "OwnedAddInManifest",
        "repair_addin",
        "cleanup_owned_addin",
        "SetFileInformationByHandle",
        "NtSetInformationFile",
        "Sha256",
        "USERPROFILE",
    ] {
        for source_path in &production_sources {
            let source = fs::read_to_string(source_path).expect("read production Rust source");
            assert!(
                !source.contains(forbidden),
                "production source {} contains forbidden direct-mutation token: {forbidden}",
                source_path.display()
            );
        }
    }

    let addin_source = fs::read_to_string(source_root.join("addin_install.rs"))
        .expect("read Add-In production source");
    for forbidden_write_api in ["remove_file", "fs::write", "OpenOptions", "create_dir"] {
        assert!(
            !addin_source.contains(forbidden_write_api),
            "production Add-In flow still contains filesystem mutation API: {forbidden_write_api}"
        );
    }
}

fn production_rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut sources = Vec::new();

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("read production source directory") {
            let path = entry.expect("read production source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(path);
            }
        }
    }

    sources.sort();
    sources
}

#[test]
fn process_enumeration_failure_is_unknown_and_requires_restart_for_a_change() {
    let state = arcgis_process_state_with(|| Err(()));

    assert_eq!(state, ArcGisProcessState::Unknown);
    assert!(state.requires_restart_for_change(true));
}

#[test]
fn launch_uses_the_validated_executable_directly_without_shell_or_arguments() {
    let executable = PathBuf::from(r"C:\ArcGIS & calc.exe\bin\ArcGISPro.exe");
    let installation = ArcGisInstallation {
        root: PathBuf::from(r"C:\ArcGIS & calc.exe"),
        executable: executable.clone(),
        version: Some("3.7.0.0".to_owned()),
        source: ArcGisInstallSource::Saved,
    };

    let command = arcgis_launch_command(&installation);
    assert_eq!(command.get_program(), executable.as_os_str());
    assert_eq!(command.get_args().count(), 0);
}

#[test]
fn manual_selection_accepts_only_a_valid_arcgispro_executable() {
    let root = TestDir::new();
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).expect("create bin directory");
    let executable = bin.join("ArcGISPro.exe");
    fs::write(&executable, b"test executable").expect("write executable");
    fs::write(bin.join("ArcGIS.Core.dll"), b"test core").expect("write core library");

    let selected = choose_arcgis_executable_with(&executable, |_| Some("3.7.0.0".to_owned()))
        .expect("accept valid ArcGISPro.exe");
    assert_eq!(selected.source, ArcGisInstallSource::Manual);
    assert!(
        choose_arcgis_executable_with(&bin.join("Other.exe"), |_| Some("3.7.0.0".to_owned()))
            .is_err()
    );
}

#[cfg(windows)]
fn create_file_symlink(link: &Path, target: &Path) {
    create_windows_link(link, target, false);
}

#[cfg(windows)]
fn create_directory_junction(link: &Path, target: &Path) {
    create_windows_link(link, target, true);
}

#[cfg(windows)]
fn create_windows_link(link: &Path, target: &Path, junction: bool) {
    let mut command = ProcessCommand::new("cmd.exe");
    command.args(["/d", "/c", "mklink"]);
    if junction {
        command.arg("/J");
    }
    let output = command.arg(link).arg(target).output().expect("run mklink");
    assert!(
        output.status.success(),
        "mklink failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
