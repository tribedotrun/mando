use crate::types::{UiAutoLaunchOptions, UiLaunchSpec};

pub(crate) fn build_launch_spec(options: &UiAutoLaunchOptions) -> UiLaunchSpec {
    let mut env = std::collections::HashMap::new();
    env.insert("MANDO_EXTERNAL_GATEWAY".to_string(), "1".to_string());
    env.insert(
        "MANDO_GATEWAY_PORT".to_string(),
        options.gateway_port.to_string(),
    );
    env.insert("MANDO_AUTH_TOKEN".to_string(), options.auth_token.clone());

    if let Some(value) = &options.app_mode {
        env.insert("MANDO_APP_MODE".to_string(), value.clone());
    }
    if let Some(value) = &options.data_dir {
        env.insert("MANDO_DATA_DIR".to_string(), value.clone());
    }
    if let Some(value) = &options.log_dir {
        env.insert("MANDO_LOG_DIR".to_string(), value.clone());
    }
    if let Some(value) = &options.disable_security_warnings {
        env.insert(
            "ELECTRON_DISABLE_SECURITY_WARNINGS".to_string(),
            value.clone(),
        );
    }
    if options.headless {
        env.insert("MANDO_HEADLESS".to_string(), "1".to_string());
    }

    let mut args = vec![options.entrypoint.clone()];
    if let Some(value) = &options.inspect_port {
        args.push(format!("--inspect={value}"));
    }
    if let Some(value) = &options.cdp_port {
        args.push(format!("--remote-debugging-port={value}"));
    }

    UiLaunchSpec {
        exec_path: options.exec_path.clone(),
        args,
        cwd: None,
        env,
    }
}
