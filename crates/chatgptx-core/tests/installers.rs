use chatgptx_core::install::{
    APP_BINARY, APP_BUNDLE_ID, InstallOptions, MANAGER_BINARY, MANAGER_BUNDLE_ID, SILENT_BINARY,
    SILENT_BUNDLE_ID, app_bundle_names, build_macos_app_bundle, build_windows_entrypoint_plan,
    companion_binary_path_from_exe, default_install_root_strategy,
    macos_companion_bundle_identifier_from_exe, shortcut_names,
};
use std::path::{Path, PathBuf};

#[test]
fn windows_entrypoint_plan_has_one_gui_executable_and_shortcut() {
    let options = InstallOptions {
        install_root: Some("C:/Users/A/Desktop".into()),
        launcher_path: Some("C:/Tools/chatgptx.exe".into()),
        manager_path: Some("C:/Tools/chatgptx-manager.exe".into()),
        remove_owned_data: false,
    };
    let plan = build_windows_entrypoint_plan(&options);
    assert!(plan.silent_shortcut.ends_with("ChatGPTX.lnk"));
    assert_eq!(plan.silent_shortcut, plan.manager_shortcut);
    assert_eq!(plan.launcher_path, "C:/Tools/chatgptx.exe");
    assert_eq!(plan.launcher_path, plan.manager_path);
    assert_eq!(plan.silent_icon_path, plan.manager_icon_path);
    assert_eq!(plan.uninstall_key, "ChatGPTX");
    assert_eq!(plan.legacy_uninstall_key, "ChatGPTX");
    assert_eq!(
        plan.uninstaller_path.replace('\\', "/"),
        "C:/Tools/uninstall.exe"
    );
    assert_eq!(
        plan.uninstall_command.replace('\\', "/"),
        "\"C:/Tools/uninstall.exe\""
    );
    assert_eq!(
        plan.quiet_uninstall_command.replace('\\', "/"),
        "\"C:/Tools/uninstall.exe\" /S"
    );
}

#[test]
fn windows_entrypoint_plan_keeps_owned_data_removal_option() {
    let options = InstallOptions {
        remove_owned_data: true,
        ..InstallOptions::default()
    };
    let plan = build_windows_entrypoint_plan(&options);
    assert!(plan.remove_owned_data);
    assert_eq!(plan.silent_shortcut, plan.manager_shortcut);
    assert_eq!(plan.launcher_path, plan.manager_path);
}

#[test]
fn macos_bundle_metadata_has_one_native_gui_app_with_url_protocols() {
    let options = InstallOptions {
        install_root: Some("/Applications".into()),
        launcher_path: Some("/opt/ChatGPTX.app/Contents/MacOS/ChatGPTX".into()),
        manager_path: Some("/opt/old-manager".into()),
        remove_owned_data: false,
    };
    let app = build_macos_app_bundle(&options, false);
    assert_eq!(app, build_macos_app_bundle(&options, true));
    assert!(app.app_path.ends_with("ChatGPTX.app"));
    assert!(app.info_plist.contains("<string>ChatGPTX</string>"));
    assert!(
        app.info_plist
            .contains("<key>ChatGPTXUnifiedApp</key>\n  <true/>")
    );
    assert!(
        app.info_plist
            .contains("<string>com.chatgptx.app</string>")
    );
    assert!(app.info_plist.contains("<string>dreamskin</string>"));
    assert!(app.info_plist.contains("<string>chatgptx</string>"));
    assert!(
        app.info_plist
            .contains("<key>LSUIElement</key>\n  <false/>")
    );
    assert_eq!(app.binary_target_name.as_deref(), Some("ChatGPTX"));
    assert!(app.launch_script.is_empty());
    assert_eq!(app.binary_source, options.launcher_path);
}

#[test]
fn installer_name_helpers_and_legacy_constants_keep_api_compatibility() {
    assert_eq!(shortcut_names(), ("ChatGPTX.lnk", "ChatGPTX.lnk"));
    assert_eq!(app_bundle_names(), ("ChatGPTX.app", "ChatGPTX.app"));
    assert_eq!(SILENT_BINARY, APP_BINARY);
    assert_eq!(SILENT_BUNDLE_ID, APP_BUNDLE_ID);
    assert_eq!(MANAGER_BINARY, "chatgptx-manager");
    assert_eq!(MANAGER_BUNDLE_ID, "com.chatgptx.app.manager");
}

#[test]
fn windows_workflows_verify_nsis_and_support_both_program_files_directories() {
    for workflow in ["pr-build.yml", "release-assets.yml"] {
        let text = std::fs::read_to_string(format!("../../.github/workflows/{workflow}"))
            .unwrap()
            .replace("\r\n", "\n");
        let install = text
            .split_once("      - name: Install NSIS\n")
            .unwrap()
            .1
            .split("      - name:")
            .next()
            .unwrap();
        let build = text
            .split_once("      - name: Build Windows installer\n")
            .unwrap()
            .1
            .split("      - name:")
            .next()
            .unwrap();
        for step in [install, build] {
            assert!(step.contains("shell: pwsh"), "{workflow}");
            assert!(
                step.contains(r"${env:ProgramFiles(x86)}\NSIS\makensis.exe"),
                "{workflow}"
            );
            assert!(
                step.contains(r"$env:ProgramFiles\NSIS\makensis.exe"),
                "{workflow}"
            );
            assert!(step.contains("Get-Command makensis.exe"), "{workflow}");
            assert!(
                step.contains("Test-Path -LiteralPath $_ -PathType Leaf"),
                "{workflow}"
            );
            assert!(step.contains("if (-not $makensis) {"), "{workflow}");
        }
        assert!(install.contains("& $makensis /VERSION"), "{workflow}");
        assert!(
            install.contains("if ($LASTEXITCODE -eq 0) { break }"),
            "{workflow}"
        );
        assert!(install.contains("$attempt -le 3"), "{workflow}");
        assert!(
            build.contains("if ($LASTEXITCODE -ne 0) { throw"),
            "{workflow}"
        );
        assert!(build.contains("} finally {"), "{workflow}");
    }
}

#[test]
fn windows_installer_writes_the_same_uninstall_key_as_the_runtime() {
    let nsi = std::fs::read_to_string("../../scripts/installer/windows/ChatGPTX.nsi").unwrap();
    assert!(nsi.contains(r"Uninstall\ChatGPTX"));
    assert!(nsi.contains(
        r#"DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPTX""#
    ));
    assert!(nsi.contains(
        r#"WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPTX""#
    ));
}

#[test]
fn windows_installer_migrates_existing_shortcuts_without_reviving_deleted_ones() {
    let nsi = std::fs::read_to_string("../../scripts/installer/windows/ChatGPTX.nsi").unwrap();
    let lf = nsi.replace("\r\n", "\n");
    for text in [&lf, &lf.replace('\n', "\r\n")] {
        let text = text.replace("\r\n", "\n");
        for key in ["ChatGPTX", "ChatGPTX"] {
            let read = format!(
                r#"ReadRegStr $ExistingInstall HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{key}" "InstallLocation""#
            );
            assert!(text.find(&read).unwrap() < text.find("CreateShortcut").unwrap());
        }
        assert!(text.contains("StrCmp $ExistingInstall \"\" desktop_entry_create 0"));
        assert!(text.contains("StrCmp $HadDesktopEntry 1 desktop_entry_create desktop_entry_done"));
        assert!(
            text.contains(r#"IfFileExists "$DESKTOP\ChatGPTX 管理工具.lnk" desktop_entry_found 0"#)
        );
        assert!(
            text.contains(
                r#"CreateShortcut "$DESKTOP\ChatGPTX.lnk" "$INSTDIR\chatgptx.exe""#
            )
        );
        assert!(text.contains(r#"CreateShortcut "$SMPROGRAMS\ChatGPTX\ChatGPTX.lnk""#));
        assert!(!text.contains(r#"CreateShortcut "$DESKTOP\ChatGPTX 管理工具.lnk""#));
        assert!(!text.contains(r#"File "${ROOT}\dist\windows\app\chatgptx-manager.exe""#));
    }
}

#[test]
fn windows_installer_waits_for_normal_exit_and_cancels_without_writing_apps() {
    let nsi = std::fs::read_to_string("../../scripts/installer/windows/ChatGPTX.nsi")
        .unwrap()
        .replace("\r\n", "\n");
    assert!(!nsi.contains("taskkill"));
    assert!(nsi.contains("IntCmp $2 20 wait_prompt 0 wait_prompt"));
    assert!(nsi.contains("MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION"));
    assert!(nsi.contains("IDRETRY wait_retry IDCANCEL wait_cancel"));
    assert!(nsi.contains("wait_cancel:\n    Abort"));
    for (name, wait) in [
        ("Install", "Call WaitForProcessExit"),
        ("Uninstall", "Call un.WaitForProcessExit"),
    ] {
        let section = nsi
            .split_once(&format!("Section \"{name}\""))
            .unwrap()
            .1
            .split_once("SectionEnd")
            .unwrap()
            .0;
        let last_wait = section.rfind(wait).unwrap();
        let first_write = section.find("\n  Delete ").unwrap();
        assert!(last_wait < first_write);
        if let Some(file) = section.find("\n  File ") {
            assert!(last_wait < file);
        }
    }
}

#[test]
fn macos_dmg_includes_one_visible_app_and_hidden_legacy_updater() {
    let script = std::fs::read_to_string("../../scripts/installer/macos/package-dmg.sh").unwrap();
    assert!(script.contains("ln -s /Applications \"$STAGE/Applications\""));
    assert!(script.contains("ChatGPTX.app"));
    // The legacy updater still looks up this exact companion path. It is
    // intentionally shipped as a hidden, self-exiting compatibility shim.
    assert!(script.contains("ChatGPTX 管理工具.app"));
    assert!(script.contains("chatgptx-legacy-shim"));
    let app_commands: Vec<_> = script
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("create_app "))
        .collect();
    assert_eq!(
        app_commands,
        [
            r#"create_app "ChatGPTX" "ChatGPTX" "$BINARY_DIR/chatgptx" "com.chatgptx.app" "false""#,
            r#"create_app "ChatGPTX 管理工具" "ChatGPTXManager" "$BINARY_DIR/chatgptx-legacy-shim" "com.chatgptx.app.manager" "true""#,
        ]
    );
    assert!(script.contains("printf '%s\\n' \"ChatGPTX 管理工具.app\" > \"$STAGE/.hidden\""));
    assert!(script.contains("chflags hidden \"$STAGE/ChatGPTX 管理工具.app\""));
    assert!(script.contains("set visible of item \"ChatGPTX 管理工具.app\" to false"));
}

#[test]
fn both_companion_names_resolve_to_current_unified_app() {
    let exe = Path::new("/Applications/ChatGPTX.app/Contents/MacOS/ChatGPTX");
    for binary in [APP_BINARY, MANAGER_BINARY] {
        assert_eq!(companion_binary_path_from_exe(exe, binary), exe);
        assert_eq!(
            macos_companion_bundle_identifier_from_exe(exe, binary),
            Some(APP_BUNDLE_ID)
        );
    }
}

#[test]
fn legacy_macos_layout_redirects_companions_to_unified_app() {
    for old in [
        "/Applications/ChatGPTX.app/Contents/MacOS/ChatGPTX",
        "/Applications/ChatGPTX 管理工具.app/Contents/MacOS/ChatGPTXManager",
    ] {
        for binary in [APP_BINARY, MANAGER_BINARY] {
            assert!(
                companion_binary_path_from_exe(Path::new(old), binary)
                    .to_string_lossy()
                    .replace('\\', "/")
                    .eq_ignore_ascii_case("/Applications/ChatGPTX.app/Contents/MacOS/ChatGPTX")
            );
            assert_eq!(
                macos_companion_bundle_identifier_from_exe(Path::new(old), binary),
                Some(APP_BUNDLE_ID)
            );
        }
    }
}

#[test]
fn translocated_unified_app_uses_stable_bundle_id() {
    let exe = Path::new(
        "/private/var/folders/x/AppTranslocation/id/d/ChatGPTX.app/Contents/MacOS/ChatGPTX",
    );
    assert_eq!(
        macos_companion_bundle_identifier_from_exe(exe, MANAGER_BINARY),
        Some(APP_BUNDLE_ID)
    );
}

#[test]
fn bare_unified_executable_resolves_both_roles_to_itself() {
    let exe = Path::new("/tmp/target/debug/chatgptx");
    for binary in [APP_BINARY, MANAGER_BINARY] {
        assert_eq!(companion_binary_path_from_exe(exe, binary), exe);
        assert_eq!(
            macos_companion_bundle_identifier_from_exe(exe, binary),
            None
        );
    }
}

#[test]
fn default_install_root_uses_platform_strategy() {
    assert_eq!(
        default_install_root_strategy(),
        if cfg!(windows) {
            "windows-known-folder"
        } else if cfg!(target_os = "macos") {
            "macos-applications"
        } else {
            "user-dirs-desktop"
        }
    );
}

#[cfg(target_os = "macos")]
mod macos_install {
    use super::*;

    fn write_native_app(root: &Path) -> Vec<PathBuf> {
        let contents = root.join("ChatGPTX.app/Contents");
        let binary = contents.join("MacOS/ChatGPTX");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        let mut bytes = vec![0_u8; 2048];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        std::fs::write(&binary, bytes).unwrap();
        std::fs::create_dir_all(contents.join("_CodeSignature")).unwrap();
        std::fs::create_dir_all(contents.join("Resources")).unwrap();
        let plist = contents.join("Info.plist");
        std::fs::write(&plist, "<plist><dict><key>ChatGPTXUnifiedApp</key><true/><key>CFBundleName</key><string>ChatGPTX</string><key>CFBundleIdentifier</key><string>com.chatgptx.app</string><key>CFBundleExecutable</key><string>ChatGPTX</string><key>CFBundleIconFile</key><string>native.icns</string><key>LSUIElement</key><false/></dict></plist>").unwrap();
        let signature = contents.join("_CodeSignature/CodeResources");
        std::fs::write(&signature, b"sealed resource sentinel").unwrap();
        let icon = contents.join("Resources/native.icns");
        std::fs::write(&icon, b"native icon sentinel").unwrap();
        vec![plist, binary, signature, icon]
    }

    fn options(root: &Path, source: &Path) -> InstallOptions {
        InstallOptions {
            install_root: Some(root.to_path_buf()),
            launcher_path: Some(source.to_path_buf()),
            ..InstallOptions::default()
        }
    }

    fn snapshot(paths: &[PathBuf]) -> Vec<Vec<u8>> {
        paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect()
    }

    #[test]
    fn repair_preserves_native_app_and_sealed_files_without_a_manager() {
        let root = tempfile::tempdir().unwrap();
        let files = write_native_app(root.path());
        let before = snapshot(&files);
        let plan = options(root.path(), &files[1]);
        for _ in 0..2 {
            chatgptx_core::install::macos::install_app_bundles(&plan).unwrap();
            assert_eq!(snapshot(&files), before);
        }
        assert!(
            !root
                .path()
                .join("ChatGPTX.app/Contents/MacOS/chatgptx-launcher")
                .exists()
        );
        assert!(!root.path().join("ChatGPTX 管理工具.app").exists());
    }

    #[test]
    fn repair_preserves_native_app_through_symlink_and_hard_link_aliases() {
        let root = tempfile::tempdir().unwrap();
        let files = write_native_app(root.path());
        let before = snapshot(&files);
        let symlink = root.path().join("exe-symlink");
        let hard_link = root.path().join("exe-hard-link");
        std::os::unix::fs::symlink(&files[1], &symlink).unwrap();
        std::fs::hard_link(&files[1], &hard_link).unwrap();
        for alias in [symlink, hard_link] {
            chatgptx_core::install::macos::install_app_bundles(&options(root.path(), &alias))
                .unwrap();
            assert_eq!(snapshot(&files), before);
        }
    }

    #[test]
    fn install_copies_complete_native_app_without_rebuilding_its_files() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let files = write_native_app(source.path());
        let before = snapshot(&files);
        chatgptx_core::install::macos::install_app_bundles(&options(target.path(), &files[1]))
            .unwrap();
        let copied = files
            .iter()
            .map(|path| {
                target
                    .path()
                    .join(path.strip_prefix(source.path()).unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(snapshot(&copied), before);
        assert_eq!(snapshot(&files), before);
        assert!(!target.path().join("ChatGPTX 管理工具.app").exists());
    }

    #[test]
    fn renamed_gui_bundle_wins_over_same_named_legacy_sibling() {
        let root = tempfile::tempdir().unwrap();
        let files = write_native_app(root.path());
        let before_gui = snapshot(&files);
        let original = root.path().join("ChatGPTX.app");
        let renamed = root.path().join("ChatGPTX 2.app");
        std::fs::rename(root.path().join("ChatGPTX.app"), &renamed).unwrap();
        let current = renamed.join("Contents/MacOS/ChatGPTX");
        let legacy = write_native_app(root.path());
        let plist = std::fs::read_to_string(&legacy[0]).unwrap().replace(
            "<key>ChatGPTXUnifiedApp</key><true/>",
            "<key>ChatGPTXUnifiedApp</key><false/>",
        );
        std::fs::write(&legacy[0], plist).unwrap();
        let before = snapshot(&legacy);
        assert_eq!(
            chatgptx_core::install::macos::native_application_bundle_from_executable(&current)
                .unwrap(),
            std::fs::canonicalize(&renamed).unwrap(),
        );
        for binary in [APP_BINARY, MANAGER_BINARY] {
            assert_eq!(companion_binary_path_from_exe(&current, binary), current);
        }
        assert!(
            chatgptx_core::install::macos::native_application_bundle_from_executable(&legacy[1])
                .is_err()
        );
        assert_eq!(snapshot(&legacy), before);
        let renamed_files = files
            .iter()
            .map(|path| renamed.join(path.strip_prefix(&original).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(snapshot(&renamed_files), before_gui);
    }

    #[test]
    fn native_gui_bundle_helper_rejects_auxiliary_executable_inside_the_bundle() {
        let root = tempfile::tempdir().unwrap();
        let files = write_native_app(root.path());
        let auxiliary = files[1].with_file_name("auxiliary");
        std::fs::copy(&files[1], &auxiliary).unwrap();
        let error =
            chatgptx_core::install::macos::native_application_bundle_from_executable(&auxiliary)
                .unwrap_err();
        assert!(error.to_string().contains("不是 ChatGPTX 原生界面的主程序"));
    }

    #[test]
    fn bare_binary_cannot_create_a_partial_or_unsigned_wrapper_app() {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("chatgptx");
        std::fs::write(&binary, vec![0_u8; 2048]).unwrap();
        let target = root.path().join("apps");
        let error =
            chatgptx_core::install::macos::install_app_bundles(&options(&target, &binary))
                .unwrap_err();
        assert!(error.to_string().contains("完整的 ChatGPTX.app"));
        assert!(!target.exists());
    }

    #[test]
    fn legacy_silent_app_with_same_bundle_id_cannot_be_installed_as_gui() {
        let root = tempfile::tempdir().unwrap();
        let files = write_native_app(root.path());
        let plist = std::fs::read_to_string(&files[0]).unwrap().replace(
            "<key>ChatGPTXUnifiedApp</key><true/>",
            "<key>ChatGPTXUnifiedApp</key><false/>",
        );
        std::fs::write(&files[0], plist).unwrap();
        let before = snapshot(&files);
        let error =
            chatgptx_core::install::macos::install_app_bundles(&options(root.path(), &files[1]))
                .unwrap_err();
        assert!(error.to_string().contains("ChatGPTX 原生界面包"));
        assert_eq!(snapshot(&files), before);
    }

    #[test]
    fn other_native_source_cannot_overwrite_installed_signed_app() {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let files = write_native_app(root.path());
        let other = write_native_app(source.path());
        let before = snapshot(&files);
        let error =
            chatgptx_core::install::macos::install_app_bundles(&options(root.path(), &other[1]))
                .unwrap_err();
        assert!(error.to_string().contains("不会改写"));
        assert_eq!(snapshot(&files), before);
    }
}
