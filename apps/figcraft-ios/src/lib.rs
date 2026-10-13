//! Native FigCraft iPadOS runner: the Rust static library the Xcode harness in `packaging/ios`
//! links. UIKit startup and the Metal surface lifecycle belong to winit and eframe; this crate adds
//! the single UIKit scene, safe-area insets, document pickers and the browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_os = "ios")]
mod app;
#[cfg(target_os = "ios")]
#[allow(unsafe_code)]
mod apple_entry;
#[cfg(any(target_os = "ios", test))]
mod documents;
#[cfg(any(target_os = "ios", test))]
mod logging;
#[cfg(target_os = "ios")]
mod safe_area;
#[cfg(target_os = "ios")]
#[allow(unsafe_code)]
mod scenes;
#[cfg(target_os = "ios")]
mod services;
#[cfg(any(target_os = "ios", test))]
mod storage;

/// Configure Metal explicitly; UIKit/winit supplies the current window dimensions.
pub fn native_options() -> eframe::NativeOptions {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = eframe::wgpu::Backends::METAL;
    setup.device_descriptor = std::sync::Arc::new(|adapter| {
        let available = adapter.limits();
        let requested = eframe::wgpu::Limits { max_texture_dimension_2d: 8192, ..Default::default() };
        eframe::wgpu::DeviceDescriptor {
            label: Some("figcraft iOS Metal device"),
            required_limits: compatible_limits(requested, &available),
            ..Default::default()
        }
    });
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_app_id("ai.storyteller.figcraft").with_title("FigCraft"),
        renderer: eframe::Renderer::Wgpu,
        run_and_return: false,
        wgpu_options: eframe::WgpuConfiguration { wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(setup), ..Default::default() },
        ..Default::default()
    }
}

/// egui uses two inter-stage locations; Metal simulators may expose 15 instead of wgpu's default
/// 16. Keep every other limit and the default feature set.
fn compatible_limits(mut requested: eframe::wgpu::Limits, available: &eframe::wgpu::Limits) -> eframe::wgpu::Limits {
    // Below two, device creation must fail rather than pretend the egui shader is supported.
    requested.max_inter_stage_shader_variables = requested.max_inter_stage_shader_variables.min(available.max_inter_stage_shader_variables).max(2);
    requested
}

/// Launch on iOS's main thread; other platforms return an error without a window.
pub fn run() -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        use std::sync::{Arc, Mutex};

        use figcraft_engine::Session;
        use figcraft_ui_egui::FigApp;
        use serde_json::json;

        if let Err(error) = logging::install() {
            eprintln!("FigCraft logging unavailable: {error}");
        }
        log::info!("Starting FigCraft iPadOS with Metal");
        scenes::register();
        let root = match services::create() {
            Ok(platform) => Some(platform.root),
            Err(error) => {
                log::error!("iOS platform services unavailable: {error}");
                None
            }
        };
        eframe::run_native(
            "FigCraft",
            native_options(),
            Box::new(move |cc| {
                scenes::attach(cc)?;
                // Hide the status bar through the safe platform API so it cannot overlay egui's menu.
                use winit::platform::ios::WindowExtIOS;
                if let Some(window) = cc.winit_window() {
                    window.set_prefers_status_bar_hidden(true);
                }
                // Index the system fonts in the background, then wake the UI to lay text out with them.
                let ctx_slot: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(Some(cc.egui_ctx.clone())));
                let fonts_slot = ctx_slot.clone();
                std::thread::spawn(move || {
                    figcraft_text::scan_system_fonts();
                    if let Some(c) = fonts_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
                        c.request_repaint();
                    }
                });
                let mut session = Session::empty();
                // Live sessions: the network threads wake the window when something arrives.
                let wake = cc.egui_ctx.clone();
                session.live.install(Box::new(figcraft_collab_net::NetLink::new(Arc::new(move || wake.request_repaint()))));
                let prefs = root.as_deref().and_then(services::load_prefs);
                let first_run = prefs.is_none();
                if let Some(mut prefs) = prefs {
                    // Recent files that are managed copies in an earlier install of the app container.
                    if let Some(root) = &root {
                        storage::remap_recent_files(&mut prefs.recent_files, root);
                    }
                    session.prefs = prefs;
                }
                if let Some(root) = &root {
                    session.plugins.install_dir = Some(root.join("Plugins"));
                }
                // A first run (no settings kept yet) starts on the home screen; later runs in a new file.
                if session.docs.is_empty() && !first_run {
                    let _ = session.execute("file.new", &json!({}));
                }
                let mut fig = FigApp::new(session, Box::new(services::Ios::new(root.clone(), cc.egui_ctx.clone())));
                fig.ui.home = false;
                figcraft_ui_egui::windows::install(fig);
                Ok(Box::new(app::IosApp::new(root)))
            }),
        )
        .map_err(|error| format!("FigCraft iOS startup failed: {error}"))
    }
    #[cfg(not(target_os = "ios"))]
    {
        Err("FigCraft iOS runner requires an iOS device or simulator".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn simulator_varying_limit_does_not_reduce_other_limits() {
        let requested = eframe::wgpu::Limits { max_texture_dimension_2d: 8192, max_buffer_size: 512 * 1024 * 1024, ..Default::default() };
        let available = eframe::wgpu::Limits { max_inter_stage_shader_variables: 15, ..requested.clone() };
        let actual = super::compatible_limits(requested.clone(), &available);
        assert_eq!(actual, eframe::wgpu::Limits { max_inter_stage_shader_variables: 15, ..requested });
    }

    #[test]
    fn adapters_below_egui_minimum_cannot_silently_lower_shader_requirements() {
        let available = eframe::wgpu::Limits { max_inter_stage_shader_variables: 1, ..Default::default() };
        assert_eq!(super::compatible_limits(eframe::wgpu::Limits::default(), &available).max_inter_stage_shader_variables, 2);
    }

    #[test]
    fn capable_adapters_keep_the_egui_varying_requirement() {
        let requested = eframe::wgpu::Limits::default();
        let available = eframe::wgpu::Limits { max_inter_stage_shader_variables: 32, ..requested.clone() };
        assert_eq!(super::compatible_limits(requested.clone(), &available), requested);
    }

    #[test]
    fn runner_rejects_non_ios_hosts() {
        if !cfg!(target_os = "ios") {
            assert!(super::run().unwrap_err().contains("iOS"));
        }
    }

    #[test]
    fn viewport_uses_os_size_and_only_metal() {
        let options = super::native_options();
        assert_eq!(options.renderer, eframe::Renderer::Wgpu);
        assert_eq!(options.viewport.app_id.as_deref(), Some("ai.storyteller.figcraft"));
        assert!(options.viewport.inner_size.is_none());
        assert!(!options.run_and_return);
        match options.wgpu_options.wgpu_setup {
            eframe::egui_wgpu::WgpuSetup::CreateNew(setup) => {
                assert_eq!(setup.instance_descriptor.backends, eframe::wgpu::Backends::METAL);
            }
            _ => panic!("runner must create its own Metal device"),
        }
    }
}
