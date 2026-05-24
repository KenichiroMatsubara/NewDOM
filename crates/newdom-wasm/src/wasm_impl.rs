use std::num::NonZeroUsize;

use newdom_core::{
    Node, NodeId, NodeKind, SceneGraph,
    vello_bridge,
};
use slotmap::{Key, KeyData};
use vello::{
    AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
    peniko::color::AlphaColor,
    peniko::color::Srgb,
};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::util::TextureBlitter;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Debug);
}

fn node_id_to_f64(id: NodeId) -> f64 {
    id.data().as_ffi() as f64
}

fn f64_to_node_id(raw: f64) -> NodeId {
    NodeId::from(KeyData::from_ffi(raw as u64))
}

fn get_f32_prop(obj: &JsValue, key: &str) -> Option<f32> {
    let val = js_sys::Reflect::get(obj, &JsValue::from_str(key)).ok()?;
    if val.is_undefined() || val.is_null() {
        return None;
    }
    val.as_f64().map(|v| v as f32)
}

/// Owns both the Scene Graph and the GPU state for a single canvas.
#[wasm_bindgen]
pub struct NdContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    renderer: Renderer,
    target_view: wgpu::TextureView,
    blitter: TextureBlitter,
    width: u32,
    height: u32,
    scene_graph: SceneGraph,
    background: [f32; 4],
}

#[wasm_bindgen]
impl NdContext {
    /// Initialize wgpu (WebGPU) + Vello from an HTML canvas element.
    /// Returns a Promise<NdContext> because GPU requests are async.
    pub async fn init(canvas: HtmlCanvasElement) -> Result<NdContext, JsValue> {
        let width = canvas.width();
        let height = canvas.height();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| JsValue::from_str(&format!("WebGPU adapter not found: {e}")))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("newdom"),
                ..Default::default()
            })
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let mut surface_config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| JsValue::from_str("surface not supported by adapter"))?;
        surface_config.usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        surface.configure(&device, &surface_config);

        let surface_format = surface_config.format;

        let target_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("newdom_vello_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let renderer = Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: NonZeroUsize::new(1),
                pipeline_cache: None,
            },
        )
        .map_err(|e| JsValue::from_str(&format!("Vello init failed: {e}")))?;

        let blitter = TextureBlitter::new(&device, surface_format);

        log::info!("NewDOM NdContext initialised ({width}x{height}, format={surface_format:?})");

        Ok(NdContext {
            device,
            queue,
            surface,
            renderer,
            target_view,
            blitter,
            width,
            height,
            scene_graph: SceneGraph::new(),
            background: [0.0, 0.0, 0.0, 1.0],
        })
    }

    /// Create a node of the given kind string ("rect"). Returns an opaque NodeId as f64.
    pub fn nd_node_create(&mut self, kind: &str) -> f64 {
        let node_kind = match kind {
            "rect" | _ => NodeKind::Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                color: [1.0, 1.0, 1.0, 1.0],
                corner_radius: 0.0,
            },
        };
        let id = self.scene_graph.insert(Node {
            kind: node_kind,
            children: Vec::new(),
            parent: None,
        });
        node_id_to_f64(id)
    }

    /// Update props of an existing node from a JS object.
    /// Accepted keys: x, y, width, height, r, g, b, a, corner_radius.
    /// Stale IDs are silently ignored.
    pub fn nd_node_update(&mut self, raw_id: f64, props: JsValue) {
        let id = f64_to_node_id(raw_id);
        let Some(node) = self.scene_graph.get_mut(id) else {
            return;
        };
        let NodeKind::Rect { x, y, width, height, color, corner_radius } = &mut node.kind;
        if let Some(v) = get_f32_prop(&props, "x")             { *x = v; }
        if let Some(v) = get_f32_prop(&props, "y")             { *y = v; }
        if let Some(v) = get_f32_prop(&props, "width")         { *width = v; }
        if let Some(v) = get_f32_prop(&props, "height")        { *height = v; }
        if let Some(v) = get_f32_prop(&props, "r")             { color[0] = v; }
        if let Some(v) = get_f32_prop(&props, "g")             { color[1] = v; }
        if let Some(v) = get_f32_prop(&props, "b")             { color[2] = v; }
        if let Some(v) = get_f32_prop(&props, "a")             { color[3] = v; }
        if let Some(v) = get_f32_prop(&props, "corner_radius") { *corner_radius = v; }
    }

    /// Set the parent of a child node. Stale IDs are silently ignored.
    pub fn nd_node_set_parent(&mut self, child_id: f64, parent_id: f64) {
        let child = f64_to_node_id(child_id);
        let parent = f64_to_node_id(parent_id);
        self.scene_graph.set_parent(child, parent);
    }

    /// Destroy a node. Stale IDs are silently ignored.
    pub fn nd_node_destroy(&mut self, raw_id: f64) {
        let id = f64_to_node_id(raw_id);
        self.scene_graph.remove(id);
    }

    /// Begin a frame. In Phase 0 this is a no-op; mutations take effect immediately.
    pub fn nd_begin_frame(&mut self) {}

    /// End a frame — builds the Vello scene from the Scene Graph and presents to the canvas.
    pub fn nd_end_frame(&mut self) -> Result<(), JsValue> {
        let base_color = AlphaColor::<Srgb>::new(self.background);
        let scene = vello_bridge::build_scene(&self.scene_graph);
        self.present_scene(&scene, base_color)
    }

    /// Set the background clear color (components in 0.0–1.0).
    pub fn nd_set_background(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.background = [r, g, b, a];
    }

    fn present_scene(
        &mut self,
        scene: &Scene,
        base_color: AlphaColor<Srgb>,
    ) -> Result<(), JsValue> {
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
                &self.target_view,
                &RenderParams {
                    base_color,
                    width: self.width,
                    height: self.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| JsValue::from_str(&format!("render_to_texture: {e}")))?;

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout => {
                return Err(JsValue::from_str("get_current_texture: timeout"))
            }
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                return Err(JsValue::from_str("get_current_texture: surface outdated"))
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err(JsValue::from_str("get_current_texture: surface lost"))
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(JsValue::from_str("get_current_texture: validation error"))
            }
        };

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("newdom_blit"),
            });
        self.blitter
            .copy(&self.device, &mut encoder, &self.target_view, &surface_view);
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        Ok(())
    }
}
